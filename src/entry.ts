// The `jocasta` command (0387): open the work surface, `setup [--refresh <file>]`, `uninstall`, `diagnose`,
// `mcp [--boss]`, `project …`, `--version`, and `service`, the Jocasta service bare `jocasta` starts (0552). No Jocasta
// code runs inside omp (0386); omp starts `jocasta mcp` as a stdio MCP server.
// Only the router and the error types load up front: every omp pane starts a `jocasta mcp`, so each route imports
// what it runs when it runs (PM-11); the specifiers are known, the dynamic imports only defer the work.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import jocastaPackage from "../package.json" with { type: "json" };
import { CALENDAR_ENV, takeCalendarEnv } from "./shared/credentials.ts";
import { DbError } from "./shared/db.ts";
import { UserError } from "./shared/errors.ts";
import { route, USAGE } from "./shared/route.ts";

/** `<program> --version`'s version (`omp/18.10.3`, `herdr 0.9.5`), or "not found". */
function versionOf(program: string | undefined): string {
	if (!program) return "not found";
	try {
		const out = Bun.spawnSync([program, "--version"], { stdout: "pipe", stderr: "ignore", timeout: 10_000 });
		const text = out.stdout.toString().trim();
		return out.exitCode === 0 && text ? (text.split(/[\s/]/).pop() ?? text) : "not found";
	} catch {
		return "not found";
	}
}

/** Runs a program to completion with its output captured. */
async function run(argv: string[]): Promise<{ code: number; output: string }> {
	const child = Bun.spawn(argv, { stdout: "pipe", stderr: "pipe" });
	const [stdout, stderr, code] = await Promise.all([
		new Response(child.stdout).text(),
		new Response(child.stderr).text(),
		child.exited,
	]);
	return { code, output: `${stdout}${stderr}` };
}

/** `jocasta --version`'s line: Jocasta's, omp's, herdr's and Bun's versions. */
async function versionLine(): Promise<string> {
	const { findHerdr } = await import("./frontier/server/herdr.ts");
	return `jocasta ${jocastaPackage.version} (omp ${versionOf(Bun.which("omp") ?? undefined)}, herdr ${versionOf(findHerdr())}, bun ${Bun.version})`;
}

/** Lines to the console and questions from it: one reader for every question a command asks. */
function consoleIo() {
	const lines = console[Symbol.asyncIterator]();
	return {
		print: (line: string) => console.log(line),
		ask: async (question: string) => {
			process.stdout.write(question);
			const next = await lines.next();
			return next.done ? undefined : next.value;
		},
	};
}

const target = route(process.argv.slice(2));
// The calendar's secret never reaches herdr's server, omp or anything else this command starts (SEC-5, 0430).
const calendarEnv = takeCalendarEnv();
try {
	switch (target.role) {
		case "version":
			console.log(await versionLine());
			break;
		case "help":
			console.log(USAGE);
			break;
		case "usage":
			console.error(USAGE);
			process.exitCode = 2;
			break;
		case "project": {
			const { projectCommand } = await import("./shared/cli.ts");
			for (const line of projectCommand(target.args)) console.log(line);
			break;
		}
		case "thread":
		case "worktree": {
			// Through the running service, which asks him first for the changes (0544).
			const { cliRequest, runCliCommand, runningService } = await import("./shared/thread-cli.ts");
			process.exitCode = await runCliCommand(cliRequest(target.role, target.args), {
				service: runningService,
				print: (line) => console.log(line),
				printError: (line) => console.error(line),
			});
			break;
		}
		case "mcp": {
			const { runMcpServer } = await import("./mcp/server.ts");
			// stdout carries only MCP messages.
			await runMcpServer({
				boss: target.boss,
				input: process.stdin,
				output: (line) => process.stdout.write(`${line}\n`),
				paneId: process.env.HERDR_PANE_ID,
				// Only the boss pane's environment has it (0484); a thread's `jocasta mcp` has none to send.
				...(target.boss && process.env.JOCASTA_BOSS_KEY ? { bossKey: process.env.JOCASTA_BOSS_KEY } : {}),
			});
			break;
		}
		case "service": {
			const { runService } = await import("./frontier/server/run.ts");
			process.exitCode = await runService();
			break;
		}
		case "plugin": {
			// What the `jocasta` herdr plugin runs (0553): its panes, its startup hook and its actions.
			if (target.part === "today") {
				const { runToday } = await import("./frontier/today/main.ts");
				process.exitCode = await runToday();
			} else if (target.part === "ask") {
				const { runAsk } = await import("./frontier/ask/main.ts");
				process.exitCode = await runAsk();
			} else if (target.part === "startup") {
				const [{ connectHerdr, herdrPipe }, { applyAgentView }, { herdrSession }] = await Promise.all([
					import("./frontier/server/herdr.ts"),
					import("./frontier/server/plugin.ts"),
					import("./shared/cli.ts"),
				]);
				// herdr names its socket file for its plugin commands; on Windows that is a pipe name.
				const socketFile = process.env.HERDR_SOCKET_PATH;
				const herdr = await connectHerdr({
					session: herdrSession(),
					...(socketFile ? { socketPath: herdrPipe(socketFile) } : {}),
				});
				herdr.keepDown();
				console.log(
					(await applyAgentView(herdr)) ? "Agents view set." : "No boss workspace yet; the service sets the view.",
				);
				herdr.close();
			} else if (target.part === "action") {
				// The focused pane from herdr's invocation context (`focused_pane_id`), else the pane it ran in.
				const { postAction } = await import("./frontier/today/api.ts");
				let paneId = process.env.HERDR_PANE_ID;
				try {
					const context: unknown = JSON.parse(process.env.HERDR_PLUGIN_CONTEXT_JSON ?? "{}");
					const focused = (context as { focused_pane_id?: unknown } | null)?.focused_pane_id;
					if (typeof focused === "string" && focused) paneId = focused;
				} catch {
					// No context: the pane it ran in.
				}
				const refused = await postAction(target.action, paneId);
				if (refused) {
					console.error(`jocasta: ${refused}`);
					process.exitCode = 1;
				}
			}
			break;
		}
		case "setup": {
			const [setup, { findHerdr }, { selfCommand, shippedConfigDir }, { herdrSession }, runValue] = await Promise.all([
				import("./setup/setup.ts"),
				import("./frontier/server/herdr.ts"),
				import("./shared/install.ts"),
				import("./shared/cli.ts"),
				import("./setup/run-value.ts"),
			]);
			const herdr = findHerdr();
			const paths = setup.defaultSetupPaths(shippedConfigDir(), selfCommand());
			const deps = {
				herdr,
				mise: Bun.which("mise") ?? undefined,
				run,
				calendar: setup.defaultCalendarStore(calendarEnv),
				...(herdr ? { reloadHerdr: () => run([herdr, "--session", herdrSession(), "server", "reload-config"]) } : {}),
				...(process.platform === "win32"
					? { runValue: { read: runValue.readRunValue, remove: runValue.deleteRunValue } }
					: {}),
			};
			process.exitCode =
				target.refresh === undefined
					? await setup.runSetup(paths, deps, consoleIo())
					: await setup.refreshSetup(paths, deps, consoleIo(), target.refresh);
			break;
		}
		case "uninstall": {
			const [
				{ defaultSetupPaths, miseConfigDir, setupUndo },
				uninstall,
				{ connectHerdr, findHerdr },
				{ readFrontierFile },
				{ serviceState },
				{ herdrSession },
				{ installDir, selfCommand, shippedConfigDir },
				{ composeFile, composeProject },
				{ CALENDAR_CREDENTIAL, deleteCredential, readCredential },
				{ configDir, dataDir },
			] = await Promise.all([
				import("./setup/setup.ts"),
				import("./setup/uninstall.ts"),
				import("./frontier/server/herdr.ts"),
				import("./frontier/server/frontier-file.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/cli.ts"),
				import("./shared/install.ts"),
				import("./shared/containers/index.ts"),
				import("./shared/credentials.ts"),
				import("./shared/paths.ts"),
			]);
			// Only the install this command runs from: a source run removes no install.
			const dir = installDir();
			const install = dir ? uninstall.readInstallRecord(dir, process.env.LOCALAPPDATA) : undefined;
			process.exitCode = await uninstall.runUninstall(
				{
					jocastaConfigDir: configDir(),
					dataDir: dataDir(),
					miseConfigDir: miseConfigDir(),
					composeFile: composeFile(),
					install,
					runningFromInstall: install !== undefined,
				},
				{
					serviceState: () => serviceState(readFrontierFile()),
					herdr: findHerdr(),
					session: herdrSession(),
					run,
					setupUndo: setupUndo(
						defaultSetupPaths(shippedConfigDir(), selfCommand()),
						readCredential(CALENDAR_CREDENTIAL) !== undefined,
					),
					composeDown: () =>
						run([
							"docker",
							"compose",
							"-f",
							composeFile(),
							"-p",
							composeProject(),
							"down",
							"--rmi",
							"all",
							"--volumes",
						]),
					removePathEntry: uninstall.removeUserPathEntry,
					removeRunValue: uninstall.removeRunValueFor,
					deleteCredential,
					removeAfterExit: uninstall.removeAfterExit,
					// A headless server for `plugin unlink` (0558); the session's own step stops it.
					startHerdr: async () => {
						const herdr = await connectHerdr({ session: herdrSession() });
						herdr.keepDown();
						herdr.close();
					},
				},
				consoleIo(),
			);
			break;
		}
		case "diagnose": {
			const [
				{ runDiagnose },
				{ setupToast },
				{ bossCommand, ensureBossPane, LaunchedPanes },
				{ connectHerdr },
				{ readFrontierFile },
				{ serviceState },
				{ herdrSession },
				{ shippedConfigDir },
				{ configDir, dataDir },
			] = await Promise.all([
				import("./setup/diagnose.ts"),
				import("./setup/setup.ts"),
				import("./frontier/server/boss.ts"),
				import("./frontier/server/herdr.ts"),
				import("./frontier/server/frontier-file.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/cli.ts"),
				import("./shared/install.ts"),
				import("./shared/paths.ts"),
			]);
			const file = readFrontierFile();
			const herdrConfig = join(configDir(), "herdr.toml");
			process.exitCode = await runDiagnose(
				{
					dataDir: dataDir(),
					assetsDir: shippedConfigDir(),
					omp: ["omp"],
					versions: await versionLine(),
					session: herdrSession(),
					service: async () => ({ state: await serviceState(file), file }),
					connect: () =>
						connectHerdr({ session: herdrSession(), ...(existsSync(herdrConfig) ? { configPath: herdrConfig } : {}) }),
					ensureBoss: (herdr) =>
						ensureBossPane(herdr, {
							command: bossCommand(["omp"], shippedConfigDir()),
							configDir: shippedConfigDir(),
							launched: new LaunchedPanes(),
						}),
					setupWaiting: setupToast,
				},
				(line) => console.log(line),
			);
			break;
		}
		case "surface": {
			const [
				{ readFrontierFile },
				{ findHerdr, herdrSessionAnswers, stopHerdrSession },
				{ askShutdown, serviceHealth, serviceState },
				{ attachedSurface, herdrSession, openSurface, serviceStopped },
				{ stopJocastaContainersDetached },
				{ selfCommand },
				{ configDir, dataDir },
			] = await Promise.all([
				import("./frontier/server/frontier-file.ts"),
				import("./frontier/server/herdr.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/cli.ts"),
				import("./shared/containers/resources.ts"),
				import("./shared/install.ts"),
				import("./shared/paths.ts"),
			]);
			process.exitCode = await openSurface({
				serviceHealth: () => serviceHealth(readFrontierFile()),
				// A plain Bun process outside this terminal (0552): closing the terminal mustn't end it before it has
				// stopped everything. The service takes the calendar's env fallback for itself; nothing else this command
				// starts gets it.
				startService: () => {
					const [program, ...args] = selfCommand();
					if (!program) return;
					spawn(program, [...args, "service"], {
						detached: true,
						stdio: "ignore",
						windowsHide: true,
						env: { ...process.env, ...(calendarEnv ? { [CALENDAR_ENV]: calendarEnv } : {}) },
					}).unref();
				},
				herdr: findHerdr(),
				herdrConfig: join(configDir(), "herdr.toml"),
				attach: (argv, env) =>
					Bun.spawn(argv, { stdio: ["inherit", "inherit", "inherit"], env: { ...process.env, ...env } }).exited,
				print: (line) => console.error(line),
				enterSurface: () => attachedSurface(join(dataDir(), "surfaces")),
				onTerminalClose: (handler) => {
					process.on("SIGHUP", handler);
					return () => process.off("SIGHUP", handler);
				},
				tellServiceClosed: async () => {
					const file = readFrontierFile();
					return file !== undefined && (await askShutdown(file));
				},
				// A closing console leaves seconds (LC-7): herdr gets 3 of them.
				stopHerdr: async () => {
					await stopHerdrSession({ session: herdrSession(), timeoutMs: 3_000 });
				},
				stopContainers: stopJocastaContainersDetached,
				exit: () => process.exit(0),
				watchServiceStop: () => {
					const before = readFrontierFile();
					return () => serviceStopped(before, readFrontierFile(), serviceState);
				},
				// 0518: a ping that never starts herdr; the service starts a dead server again (0431).
				herdrUp: () => herdrSessionAnswers({ session: herdrSession(), timeoutMs: 1_000 }),
			});
			break;
		}
	}
} catch (error) {
	// Loud one-line failures (0101): DB failures exit 1; refusals exit with their code, 2 for an invocation error (0140).
	if (error instanceof DbError || error instanceof UserError) {
		console.error(`jocasta: ${error.message}`);
		process.exitCode = error instanceof UserError ? error.exitCode : 1;
	} else {
		throw error;
	}
}

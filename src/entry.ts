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
import type { runProgramSync as RunProgramSync } from "./shared/process.ts";
import { route, USAGE } from "./shared/route.ts";

/** `<program> --version`'s version (`omp/18.10.3`, `herdr 0.9.5`), or "not found"; `runSync` is process.ts's. */
function versionOf(runSync: typeof RunProgramSync, program: string | null): string {
	if (!program) return "not found";
	try {
		const out = runSync([program, "--version"], { timeoutMs: 10_000 });
		const text = out.stdout.trim();
		return out.code === 0 && text ? (text.split(/[\s/]/).pop() ?? text) : "not found";
	} catch {
		return "not found";
	}
}

/** Runs a program to completion with its output captured. */
async function run(argv: string[]): Promise<{ code: number; output: string }> {
	const { runProgram } = await import("./shared/process.ts");
	const { code, stdout, stderr } = await runProgram(argv);
	return { code: code ?? 1, output: `${stdout}${stderr}` };
}

/** `jocasta --version`'s line: Jocasta's, omp's, herdr's and Bun's versions. */
async function versionLine(): Promise<string> {
	const [{ findHerdr }, { onPath, runProgramSync }] = await Promise.all([
		import("./frontier/server/herdr.ts"),
		import("./shared/process.ts"),
	]);
	return `jocasta ${jocastaPackage.version} (omp ${versionOf(runProgramSync, onPath("omp"))}, herdr ${versionOf(runProgramSync, findHerdr() ?? null)}, bun ${Bun.version})`;
}

/** Lines to the console and questions from it: one reader for every question a command asks. */
function consoleIo() {
	const lines = console[Symbol.asyncIterator]();
	const ask = async (question: string) => {
		process.stdout.write(question);
		const next = await lines.next();
		return next.done ? undefined : next.value;
	};
	return {
		print: (line: string) => console.log(line),
		ask,
		// A secret (the calendar's address, SEC-LOC-12): typed in concealed text, then its rows erased and the question
		// shown again, so neither the screen nor the scrollback keeps it.
		askSecret: async (question: string) => {
			if (!process.stdout.isTTY) return ask(question);
			process.stdout.write(`${question}\x1b[8m`);
			const next = await lines.next();
			const answer = next.done ? undefined : next.value;
			const rows = Math.max(1, Math.ceil((question.length + (answer?.length ?? 0)) / (process.stdout.columns || 80)));
			process.stdout.write(`\x1b[28m${"\x1b[1A\r\x1b[2K".repeat(rows)}${question}\n`);
			return answer;
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
			// A word that is no command is named first (0581).
			if (target.unknown !== undefined) console.error(`jocasta: unknown command "${target.unknown}".`);
			console.error(USAGE);
			process.exitCode = 2;
			break;
		case "project": {
			const { projectCommand } = await import("./shared/project-cli.ts");
			for (const line of await projectCommand(target.args)) console.log(line);
			break;
		}
		case "thread":
		case "worktree": {
			// Through the running service, which asks him first for the changes (0544).
			const [{ cliRequest, runCliCommand }, { runningService }] = await Promise.all([
				import("./shared/thread-cli.ts"),
				import("./shared/service-file.ts"),
			]);
			process.exitCode = await runCliCommand(cliRequest(target.role, target.args), {
				service: runningService,
				print: (line) => console.log(line),
				printError: (line) => console.error(line),
			});
			break;
		}
		case "mcp": {
			const [{ runMcpServer }, { BOSS_KEY_ENV }] = await Promise.all([
				import("./mcp/server.ts"),
				import("./shared/boss-key.ts"),
			]);
			const bossKey = process.env[BOSS_KEY_ENV];
			// stdout carries only MCP messages.
			await runMcpServer({
				boss: target.boss,
				input: process.stdin,
				output: (line) => process.stdout.write(`${line}\n`),
				// The pane's own key (0578): the service knows the pane by it, never by a pane id it is told.
				paneKey: process.env.JOCASTA_PANE_KEY,
				// Only the boss pane's environment has it (0484); a thread's `jocasta mcp` has none to send.
				...(target.boss && bossKey ? { bossKey } : {}),
			});
			break;
		}
		case "service": {
			const { runService } = await import("./frontier/server/run.ts");
			// Ends here: no stray handle keeps a stopped or failed service holding the relay port.
			process.exit(await runService());
			break;
		}
		case "plugin": {
			// What the `jocasta` herdr plugin runs (0553): its panes, its startup hook and its actions. A pane's process ends
			// once it returns, so no leftover handle keeps a closed popup alive.
			if (target.part === "today") {
				const { runToday } = await import("./frontier/today/main.ts");
				process.exit(await runToday());
			} else if (target.part === "ask") {
				const { runAsk } = await import("./frontier/ask/main.ts");
				process.exit(await runAsk());
			} else if (target.part === "startup") {
				const [{ connectHerdr, herdrPipe }, { applyAgentView }, { herdrSession }] = await Promise.all([
					import("./frontier/server/herdr.ts"),
					import("./frontier/server/plugin.ts"),
					import("./shared/herdr-session.ts"),
				]);
				// herdr names its socket file for its plugin commands; on Windows that is a pipe name.
				const socketFile = process.env.HERDR_SOCKET_PATH;
				// Only a running server: the hook never starts a second one (so there is nothing to revive either).
				const herdr = await connectHerdr({
					session: herdrSession(),
					start: false,
					...(socketFile ? { socketPath: herdrPipe(socketFile) } : {}),
				});
				herdr.keepDown();
				console.log(
					(await applyAgentView(herdr)) ? "Agents view set." : "No boss workspace yet; the service sets the view.",
				);
				herdr.close();
			} else if (target.part === "action") {
				// The focused pane from herdr's invocation context (`focused_pane_id`), else the pane it ran in.
				const { postAction } = await import("./frontier/client.ts");
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
			const [setup, { findHerdr }, { selfCommand, shippedConfigDir }, { herdrSession }, runValue, { onPath }] =
				await Promise.all([
					import("./setup/setup.ts"),
					import("./frontier/server/herdr.ts"),
					import("./shared/install.ts"),
					import("./shared/herdr-session.ts"),
					import("./setup/run-value.ts"),
					import("./shared/process.ts"),
				]);
			const herdr = findHerdr();
			const paths = setup.defaultSetupPaths(shippedConfigDir(), selfCommand());
			const deps = {
				herdr,
				mise: onPath("mise") ?? undefined,
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
				{ readServiceFile },
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
				import("./shared/service-file.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/herdr-session.ts"),
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
					// Windows holds the running jocasta.exe: its files go after this exits. POSIX removes them at once.
					runningFromInstall: install !== undefined && process.platform === "win32",
				},
				{
					serviceState: () => serviceState(readServiceFile()),
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
					processesUnder: uninstall.processesUnder,
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
				{ setupCheck },
				{ bossCommand, ensureBossPane, LaunchedPanes },
				{ connectHerdr },
				{ readServiceFile },
				{ serviceState },
				{ herdrSession },
				{ shippedConfigDir },
				{ configDir, dataDir },
			] = await Promise.all([
				import("./setup/diagnose.ts"),
				import("./setup/setup.ts"),
				import("./frontier/server/boss.ts"),
				import("./frontier/server/herdr.ts"),
				import("./shared/service-file.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/herdr-session.ts"),
				import("./shared/install.ts"),
				import("./shared/paths.ts"),
			]);
			const file = readServiceFile();
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
					setupCheck,
				},
				(line) => console.log(line),
			);
			break;
		}
		case "surface": {
			const [
				{ readServiceFile },
				{ findHerdr, herdrSessionAnswers, stopHerdrSession },
				{ askShutdown, serviceHealth, serviceState },
				{ herdrSession },
				{ attachedSurface, lastLoggedError, openSurface, serviceStopped },
				{ stopJocastaContainersDetached },
				{ selfCommand },
				{ configDir, dataDir },
				{ logsDir },
			] = await Promise.all([
				import("./shared/service-file.ts"),
				import("./frontier/server/herdr.ts"),
				import("./frontier/server/launch.ts"),
				import("./shared/herdr-session.ts"),
				import("./shared/surface.ts"),
				import("./shared/containers/resources.ts"),
				import("./shared/install.ts"),
				import("./shared/paths.ts"),
				import("./shared/log.ts"),
			]);
			process.exitCode = await openSurface({
				serviceHealth: () => serviceHealth(readServiceFile()),
				// A plain Bun process outside this terminal (0552): closing the terminal mustn't end it before it has
				// stopped everything. The service takes the calendar's env fallback for itself; nothing else this command
				// starts gets it.
				startService: () => {
					const [program = process.execPath, ...args] = selfCommand();
					const child = spawn(program, [...args, "service"], {
						detached: true,
						stdio: "ignore",
						windowsHide: true,
						env: { ...process.env, ...(calendarEnv ? { [CALENDAR_ENV]: calendarEnv } : {}) },
					});
					const { promise: exited, resolve } = Promise.withResolvers<number>();
					child.once("exit", (code) => resolve(code ?? 1));
					child.once("error", () => resolve(1));
					child.unref();
					return { exited };
				},
				lastError: (since) => lastLoggedError(logsDir(), since),
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
					const file = readServiceFile();
					return file !== undefined && (await askShutdown(file));
				},
				// A closing console leaves seconds (LC-7): herdr gets 3 of them.
				stopHerdr: async () => {
					await stopHerdrSession({ session: herdrSession(), timeoutMs: 3_000 });
				},
				stopContainers: stopJocastaContainersDetached,
				exit: () => process.exit(0),
				watchServiceStop: () => {
					const before = readServiceFile();
					return () => serviceStopped(before, readServiceFile(), serviceState);
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
		// A usage is printed as it is, its lines aligned (0581).
		console.error(error.message.startsWith("usage: ") ? error.message : `jocasta: ${error.message}`);
		process.exitCode = error instanceof UserError ? error.exitCode : 1;
	} else {
		throw error;
	}
}

//! Bundles the canvas page and Monaco's editor worker with Bun and generates the table `src/frontier/server/assets.rs`
//! includes (0639). The TypeScript build bundles the page with `Bun.build` (`assets.ts`: minify, browser target,
//! splitting) and Monaco's worker likewise (`scripts/build.ts`); the Rust binary embeds the same outputs, so a source
//! run and a shipped one serve the same bytes. No crates: Bun does the work, this script runs it.
//!
//! Cargo reruns this only when `src/frontier/canvas`, a source file the bundle read outside `node_modules`, the lock
//! or this file changed, so an unchanged page isn't bundled again. Bun missing, or a bundle that fails, fails the build.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::{env, fs};

/// What Bun runs: the two bundles, each file written under `<out>/page` or as `<out>/editor.worker.js`, the page's
/// table in `<out>/page.tsv` (`url<TAB>content-type<TAB>file`) and the page's source inputs in `<out>/inputs.txt`.
const BUNDLE: &str = r#"
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const [root, out] = process.argv.slice(2);
process.chdir(root);

async function bundle(config) {
	try {
		const result = await Bun.build(config);
		if (result.success) return result;
		for (const log of result.logs) console.error(String(log));
	} catch (error) {
		for (const log of error.errors ?? [error]) console.error(String(log));
	}
	process.exit(1);
}

rmSync(join(out, "page"), { recursive: true, force: true });
mkdirSync(join(out, "page"), { recursive: true });

// The page, as assets.ts builds it (0352): lazy chunks stay lazy.
const page = await bundle({
	entrypoints: ["src/frontier/canvas/index.html"],
	minify: true,
	target: "browser",
	splitting: true,
	metafile: true,
});
const files = new Map();
page.outputs.forEach((output, i) => {
	const name = basename(output.path);
	const file = join(out, "page", String(i));
	files.set(name.endsWith(".html") ? "/" : `/${name}`, { type: output.type, file, output });
});
let table = "";
for (const [url, { type, file, output }] of files) {
	await Bun.write(file, output);
	table += `${url}\t${type}\t${file}\n`;
}
writeFileSync(join(out, "page.tsv"), table);
writeFileSync(join(out, "inputs.txt"), `${Object.keys(page.metafile.inputs).join("\n")}\n`);

// Monaco's editor worker (0411), as scripts/build.ts builds it; the stylesheet its imports drag in is dead.
const worker = await bundle({
	entrypoints: ["node_modules/monaco-editor/esm/vs/editor/editor.worker.js"],
	target: "browser",
	format: "esm",
	minify: true,
	naming: { entry: "editor.worker.[ext]", asset: "[name]-[hash].[ext]" },
});
const script = worker.outputs.find((output) => output.path.endsWith(".js"));
if (!script) {
	console.error("Monaco's editor worker bundle has no script");
	process.exit(1);
}
await Bun.write(join(out, "editor.worker.js"), script);
"#;

fn fail(message: &str) -> ! {
	eprintln!("build.rs: {message}");
	exit(1);
}

fn main() {
	let root =
		PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| fail("CARGO_MANIFEST_DIR is not set")));
	let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_else(|| fail("OUT_DIR is not set")));
	for watched in ["build.rs", "src/frontier/canvas", "package.json", "bun.lock", "tsconfig.json"] {
		println!("cargo:rerun-if-changed={watched}");
	}

	let script = out.join("bundle.mjs");
	if let Err(error) = fs::write(&script, BUNDLE) {
		fail(&format!("couldn't write {}: {error}", script.display()));
	}
	run_bun(&root, &script, &out);

	// The page's own sources outside `node_modules` (a later import beyond the canvas folder counts); `node_modules`
	// changes arrive through the lock.
	let inputs = fs::read_to_string(out.join("inputs.txt")).unwrap_or_default();
	for input in inputs.lines().filter(|line| !line.is_empty() && !line.contains("node_modules")) {
		println!("cargo:rerun-if-changed={input}");
	}

	let table =
		fs::read_to_string(out.join("page.tsv")).unwrap_or_else(|error| fail(&format!("no page table: {error}")));
	let mut generated =
		String::from("/// Every file of the bundled canvas page: its URL path, content type and bytes.\n");
	generated.push_str("pub static PAGE_FILES: &[(&str, &str, &[u8])] = &[\n");
	for line in table.lines() {
		let mut parts = line.splitn(3, '\t');
		let (Some(url), Some(content_type), Some(file)) = (parts.next(), parts.next(), parts.next()) else {
			fail(&format!("a malformed line in the page table: {line:?}"));
		};
		let _ = writeln!(generated, "\t({url:?}, {content_type:?}, include_bytes!({file:?})),");
	}
	generated.push_str("];\n\n/// Monaco's editor worker, built as `scripts/build.ts` builds it (0411).\n");
	let worker = out.join("editor.worker.js");
	let _ =
		writeln!(generated, "pub static MONACO_WORKER: &[u8] = include_bytes!({:?});", worker.display().to_string());
	if let Err(error) = fs::write(out.join("assets_table.rs"), generated) {
		fail(&format!("couldn't write the assets table: {error}"));
	}
}

/// Runs the bundle script with Bun (on PATH through mise) from the package root.
fn run_bun(root: &Path, script: &Path, out: &Path) {
	let output = match Command::new("bun").arg("run").arg(script).arg(root).arg(out).current_dir(root).output() {
		Ok(output) => output,
		Err(error) => fail(&format!(
			"couldn't run `bun` ({error}). Bun bundles the canvas page and Monaco's worker for the binary (0639): \
			 install it (`mise install`) and put it on PATH"
		)),
	};
	if !output.status.success() {
		fail(&format!(
			"Bun's bundle of the canvas page failed ({}):\n{}{}",
			output.status,
			String::from_utf8_lossy(&output.stderr),
			String::from_utf8_lossy(&output.stdout)
		));
	}
}

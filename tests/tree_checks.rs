//! 0610's three syntax-tree checks over the Rust sources (0645), the twin of `test/quality/tree-checks.ts`: what clippy
//! can't see in `src`, `bench` and `evals`, read with `syn`. They fail on new cases only; today's are the baseline,
//! `tree-baseline.txt` beside this file, one key a line. Each finding is keyed by file, kind and qualified name, never
//! by line, so moving code keeps its key:
//! - `flag-parameter`: a `bool` parameter that an `if`, or the left side of `&&` or `||`, tests;
//! - `forwarding`: a function whose whole body is one call with exactly its own parameters as arguments, or a method
//!   without parameters that only calls `self.<path>()`;
//! - `undocumented-export`: a `pub` item, or a `pub` method of an inherent `impl`, without a doc comment.
//!
//! A fourth check holds 0681's one-way doors: `HumanView::for_his_eyes()`, the proof that code draws for his eyes, may be
//! called only under `src/frontier/today/` and `src/frontier/brief/`, and `Private::for_storage` only by the event
//! queue; any other caller fails the test with its file and line.
//!
//! Only the product's code counts: the walk follows `mod` declarations from the crate roots, so a `#[cfg(test)]` module
//! and its files are never read, and trait impls (their signatures are the trait's) are skipped.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{
	Attribute, BinOp, Block, Expr, ExprBinary, ExprIf, ExprMethodCall, ExprPath, FnArg, ImplItemFn, ItemConst,
	ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUnion, Pat, Signature, Stmt,
	Type, Visibility,
};

/// The crate roots the module walk starts from, relative to the package.
const ROOTS: [&str; 3] = ["src/lib.rs", "src/main.rs", "bench/main.rs"];

/// Whether `attrs` mark an item as compiled only for tests (`#[cfg(test)]`, `#[cfg(any(test, …))]`, `#[test]`).
fn test_only(attrs: &[Attribute]) -> bool {
	attrs.iter().any(|attr| {
		let path = attr.path();
		if path.segments.last().is_some_and(|segment| segment.ident == "test") {
			return true;
		}
		if !path.is_ident("cfg") {
			return false;
		}
		let tokens = attr.meta.require_list().map(|list| list.tokens.to_string()).unwrap_or_default();
		tokens == "test" || tokens.starts_with("any (test")
	})
}

fn has_doc(attrs: &[Attribute]) -> bool {
	attrs.iter().any(|attr| attr.path().is_ident("doc"))
}

/// Whether the expression names the local `name`.
struct Mentions<'a> {
	name: &'a str,
	found: bool,
}

impl<'ast> Visit<'ast> for Mentions<'_> {
	fn visit_expr_path(&mut self, path: &'ast ExprPath) {
		self.found |= path.path.is_ident(self.name);
	}
}

fn mentions(expr: &Expr, name: &str) -> bool {
	let mut visitor = Mentions { name, found: false };
	visitor.visit_expr(expr);
	visitor.found
}

/// Whether a branch tests the local `name`: an `if` on it, or it on the left of `&&` or `||`.
struct Tests<'a> {
	name: &'a str,
	found: bool,
}

impl<'ast> Visit<'ast> for Tests<'_> {
	fn visit_expr_if(&mut self, branch: &'ast ExprIf) {
		self.found |= mentions(&branch.cond, self.name);
		visit::visit_expr_if(self, branch);
	}

	fn visit_expr_binary(&mut self, binary: &'ast ExprBinary) {
		self.found |= matches!(binary.op, BinOp::And(_) | BinOp::Or(_)) && mentions(&binary.left, self.name);
		visit::visit_expr_binary(self, binary);
	}
}

/// The `bool` parameters of a function that its body tests.
fn flag_parameters(sig: &Signature, body: &Block) -> Vec<String> {
	let mut flags = Vec::new();
	for input in &sig.inputs {
		let FnArg::Typed(typed) = input else { continue };
		let (Pat::Ident(name), Type::Path(ty)) = (&*typed.pat, &*typed.ty) else { continue };
		if !ty.path.is_ident("bool") {
			continue;
		}
		let name = name.ident.to_string();
		let mut tests = Tests { name: &name, found: false };
		tests.visit_block(body);
		if tests.found {
			flags.push(name);
		}
	}
	flags
}

/// The name a call argument passes on: a parameter, possibly borrowed.
fn passed_on(arg: &Expr) -> Option<String> {
	match arg {
		Expr::Path(path) => path.path.get_ident().map(ToString::to_string),
		Expr::Reference(reference) => passed_on(&reference.expr),
		Expr::Paren(paren) => passed_on(&paren.expr),
		_ => None,
	}
}

/// Whether `expr` is `self` or a field path of it.
fn is_self_chain(expr: &Expr) -> bool {
	match expr {
		Expr::Path(path) => path.path.is_ident("self"),
		Expr::Field(field) => is_self_chain(&field.base),
		Expr::Paren(paren) => is_self_chain(&paren.expr),
		_ => false,
	}
}

/// The only expression of a body, `return`, `.await` and parentheses looked through.
fn only_expression(body: &Block) -> Option<&Expr> {
	let [Stmt::Expr(only, _)] = body.stmts.as_slice() else { return None };
	let mut expr = only;
	loop {
		expr = match expr {
			Expr::Return(returned) => returned.expr.as_deref()?,
			Expr::Await(awaited) => &awaited.base,
			Expr::Paren(paren) => &paren.expr,
			Expr::Group(group) => &group.expr,
			_ => return Some(expr),
		};
	}
}

/// Whether the function only forwards: its body is one call whose arguments are exactly its own parameters, or a method
/// without parameters that calls `self.<path>()` with none.
fn forwards(sig: &Signature, body: &Block) -> bool {
	let Some(call) = only_expression(body) else { return false };
	let mut params = Vec::new();
	let mut has_receiver = false;
	for input in &sig.inputs {
		match input {
			FnArg::Receiver(_) => has_receiver = true,
			FnArg::Typed(typed) => match &*typed.pat {
				Pat::Ident(name) => params.push(name.ident.to_string()),
				_ => return false,
			},
		}
	}
	let args: Vec<&Expr> = match call {
		Expr::Call(call) => call.args.iter().collect(),
		Expr::MethodCall(call) => {
			if params.is_empty() {
				return has_receiver && call.args.is_empty() && is_self_chain(&call.receiver);
			}
			call.args.iter().collect()
		}
		_ => return false,
	};
	if params.is_empty() || args.len() != params.len() {
		return false;
	}
	args.iter().all(|arg| passed_on(arg).is_some_and(|name| params.contains(&name)))
}

/// One file's findings, and the files its `mod` declarations name.
struct Checker<'a> {
	file: &'a str,
	/// The folder a `mod x;` of this file's top level looks in, and the inline modules entered since.
	dir: PathBuf,
	inline: Vec<String>,
	children: Vec<PathBuf>,
	findings: Vec<String>,
	/// The inherent `impl`'s type while inside one.
	owner: Option<String>,
	/// How deep inside function bodies: an item there is exported by no one.
	in_body: usize,
}

impl Checker<'_> {
	fn found(&mut self, kind: &str, name: &str) {
		self.findings.push(format!("{} {kind} {name}", self.file));
	}

	fn qualified(&self, name: &str) -> String {
		self.owner.as_ref().map_or_else(|| name.to_owned(), |owner| format!("{owner}.{name}"))
	}

	fn check_documented(&mut self, vis: &Visibility, attrs: &[Attribute], name: &str) {
		if self.in_body == 0 && matches!(vis, Visibility::Public(_)) && !has_doc(attrs) {
			let name = self.qualified(name);
			self.found("undocumented-export", &name);
		}
	}

	fn check_function(&mut self, sig: &Signature, body: &Block) {
		let name = self.qualified(&sig.ident.to_string());
		for flag in flag_parameters(sig, body) {
			self.found("flag-parameter", &format!("{name}({flag})"));
		}
		if forwards(sig, body) {
			self.found("forwarding", &name);
		}
	}

	/// The file a `mod name;` declaration names: its `#[path]`, else `name.rs` or `name/mod.rs` in the module's folder.
	fn module_file(&self, name: &str, attrs: &[Attribute], file_dir: &Path) -> Option<PathBuf> {
		let mut dir = self.dir.clone();
		dir.extend(&self.inline);
		for attr in attrs.iter().filter(|attr| attr.path().is_ident("path")) {
			let syn::Meta::NameValue(value) = &attr.meta else { continue };
			let Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(path), .. }) = &value.value else { continue };
			let mut base = file_dir.to_path_buf();
			base.extend(&self.inline);
			return Some(base.join(path.value()));
		}
		[dir.join(format!("{name}.rs")), dir.join(name).join("mod.rs")].into_iter().find(|candidate| candidate.exists())
	}
}

/// Every item the checks look at.
impl<'ast> Visit<'ast> for Checker<'_> {
	fn visit_item_mod(&mut self, item: &'ast ItemMod) {
		if !test_only(&item.attrs) {
			visit::visit_item_mod(self, item);
		}
	}

	fn visit_item_fn(&mut self, item: &'ast ItemFn) {
		if test_only(&item.attrs) {
			return;
		}
		self.check_documented(&item.vis, &item.attrs, &item.sig.ident.to_string());
		self.check_function(&item.sig, &item.block);
		self.in_body += 1;
		visit::visit_item_fn(self, item);
		self.in_body -= 1;
	}

	fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_union(&mut self, item: &'ast ItemUnion) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_type(&mut self, item: &'ast ItemType) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_const(&mut self, item: &'ast ItemConst) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_static(&mut self, item: &'ast ItemStatic) {
		if !test_only(&item.attrs) {
			self.check_documented(&item.vis, &item.attrs, &item.ident.to_string());
		}
	}

	fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
		if item.trait_.is_some() || test_only(&item.attrs) {
			return;
		}
		let owner = match &*item.self_ty {
			Type::Path(path) => path.path.segments.last().map(|segment| segment.ident.to_string()),
			_ => None,
		};
		self.owner = Some(owner.unwrap_or_else(|| "<type>".to_owned()));
		visit::visit_item_impl(self, item);
		self.owner = None;
	}

	fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
		if test_only(&item.attrs) {
			return;
		}
		self.check_documented(&item.vis, &item.attrs, &item.sig.ident.to_string());
		self.check_function(&item.sig, &item.block);
		self.in_body += 1;
		visit::visit_impl_item_fn(self, item);
		self.in_body -= 1;
	}
}

/// The findings of one source text, keyed under the file name `at`, and the module files it declares (found from
/// `file_dir`, the folder of the file itself, and `dir`, the folder its child modules live in).
fn read_source(at: &str, source: &str, dir: &Path, file_dir: &Path) -> (Vec<String>, Vec<PathBuf>) {
	let file = syn::parse_file(source).unwrap_or_else(|error| panic!("{at} doesn't parse: {error}"));
	let mut checker = Checker {
		file: at,
		dir: dir.to_path_buf(),
		inline: Vec::new(),
		children: Vec::new(),
		findings: Vec::new(),
		owner: None,
		in_body: 0,
	};
	checker.visit_file(&file);
	checker.visit_modules(&file.items, file_dir);
	(checker.findings, checker.children)
}

impl Checker<'_> {
	/// Follows the `mod` declarations in `items`, inline modules too; a test-only module is left alone.
	fn visit_modules(&mut self, items: &[syn::Item], file_dir: &Path) {
		for item in items {
			let syn::Item::Mod(module) = item else { continue };
			self.visit_module(module, file_dir);
		}
	}

	fn visit_module(&mut self, module: &ItemMod, file_dir: &Path) {
		if test_only(&module.attrs) {
			return;
		}
		let name = module.ident.to_string();
		match &module.content {
			Some((_, items)) => {
				self.inline.push(name);
				self.visit_modules(items, file_dir);
				self.inline.pop();
			}
			None => self.children.extend(self.module_file(&name, &module.attrs, file_dir)),
		}
	}
}

/// Every finding in the product's Rust sources under `root`, sorted.
fn tree_findings(root: &Path) -> Vec<String> {
	let mut findings = Vec::new();
	let mut seen = HashSet::new();
	let mut queue: Vec<PathBuf> = ROOTS.iter().map(|path| root.join(path)).filter(|path| path.exists()).collect();
	while let Some(path) = queue.pop() {
		let path = fs::canonicalize(&path).expect("resolving a module file");
		if !seen.insert(path.clone()) {
			continue;
		}
		let at = path
			.strip_prefix(fs::canonicalize(root).expect("resolving the package"))
			.expect("a module file inside the package")
			.to_string_lossy()
			.replace('\\', "/");
		let file_dir = path.parent().expect("a file has a folder").to_path_buf();
		let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
		let own_folder = matches!(stem, "mod" | "lib" | "main");
		let dir = if own_folder { file_dir.clone() } else { file_dir.join(stem) };
		let source = fs::read_to_string(&path).expect("reading a module file");
		let (found, children) = read_source(&at, &source, &dir, &file_dir);
		findings.extend(found);
		queue.extend(children);
	}
	findings.sort();
	findings
}

/// The keys of `found` beyond what `baseline` holds, counting repeats.
fn beyond(found: &[String], baseline: &[&str]) -> Vec<String> {
	let mut left: HashMap<&str, i64> = HashMap::new();
	for key in baseline {
		*left.entry(key).or_default() += 1;
	}
	found
		.iter()
		.filter(|key| {
			let count = left.entry(key.as_str()).or_default();
			*count -= 1;
			*count < 0
		})
		.cloned()
		.collect()
}

#[test]
fn the_rust_sources_have_no_flag_parameter_forwarding_function_or_undocumented_export_beyond_the_baseline() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let baseline = fs::read_to_string(root.join("tests").join("tree-baseline.txt")).expect("reading the baseline");
	let baseline: Vec<&str> = baseline.lines().filter(|line| !line.is_empty()).collect();
	let new = beyond(&tree_findings(root), &baseline);
	assert!(new.is_empty(), "new cases beyond tests/tree-baseline.txt:\n{}", new.join("\n"));
}

#[test]
fn each_check_finds_its_case_and_a_commented_branch_free_working_function_passes() {
	let source = [
		"pub fn loud(text: &str, shout: bool) -> String {",
		"\tif shout { text.to_uppercase() } else { text.to_owned() }",
		"}",
		"/// Says it.",
		"pub struct Speaker;",
		"impl Speaker {",
		"\tpub fn say(&self, text: &str) -> String {",
		"\t\tloud(text, false)",
		"\t}",
		"\t/// The same, louder.",
		"\tpub fn yell(&self, text: &str) -> String {",
		"\t\tself.say(text)",
		"\t}",
		"}",
		"/// Trimmed.",
		"pub fn trimmed(text: &str) -> &str {",
		"\ttext.trim()",
		"}",
		"impl Drop for Speaker {",
		"\tfn drop(&mut self) {}",
		"}",
		"#[cfg(test)]",
		"mod tests {",
		"\tpub fn helper(flag: bool) -> bool { if flag { true } else { false } }",
		"}",
	]
	.join("\n");
	let (mut found, children) = read_source("a.rs", &source, Path::new("."), Path::new("."));
	found.sort();
	assert_eq!(
		found,
		[
			"a.rs flag-parameter loud(shout)",
			"a.rs forwarding Speaker.yell",
			"a.rs undocumented-export Speaker.say",
			"a.rs undocumented-export loud",
		]
	);
	assert!(children.is_empty());
}

/// Where each of 0681's one-way doors may be used: the function's name, and the files (or folders, ending in `/`) that
/// may use it. `src/shared/private.rs` defines both and tests them.
const DOORS: [(&str, &[&str]); 2] = [
	("for_his_eyes", &["src/frontier/today/", "src/frontier/brief/", "src/shared/private.rs"]),
	("for_storage", &["src/shared/events/queue.rs", "src/shared/private.rs"]),
];

/// Whether a function named `name` is called or named in a file.
struct Names<'a> {
	name: &'a str,
	found: bool,
}

impl<'ast> Visit<'ast> for Names<'_> {
	fn visit_path(&mut self, path: &'ast syn::Path) {
		self.found |= path.segments.last().is_some_and(|segment| segment.ident == self.name);
		visit::visit_path(self, path);
	}

	fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
		self.found |= call.method == self.name;
		visit::visit_expr_method_call(self, call);
	}
}

/// The `.rs` files under `dir`, as paths relative to `root` with `/`.
fn rust_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
	let Ok(entries) = fs::read_dir(dir) else { return };
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			rust_files(root, &path, out);
		} else if path.extension().is_some_and(|ext| ext == "rs") {
			let relative = path.strip_prefix(root).expect("a file inside the package");
			out.push(relative.to_string_lossy().replace('\\', "/"));
		}
	}
}

/// Every use of a door outside the files that may use it, as `<file>:<line>: <name>`. `syn` finds the files that
/// name the function (a call, a path or a method, whatever the alias in front of it); the lines come from the text.
fn door_violations(root: &Path) -> Vec<String> {
	let mut files = Vec::new();
	for dir in ["src", "bench", "evals", "tests"] {
		rust_files(root, &root.join(dir), &mut files);
	}
	files.sort();
	let mut found = Vec::new();
	for file in files {
		let source = fs::read_to_string(root.join(&file)).expect("reading a source file");
		for (name, allowed) in DOORS {
			if allowed.iter().any(|allowed| file == *allowed || (allowed.ends_with('/') && file.starts_with(allowed))) {
				continue;
			}
			if !source.contains(name) {
				continue;
			}
			let tree = syn::parse_file(&source).unwrap_or_else(|error| panic!("{file} doesn't parse: {error}"));
			let mut names = Names { name, found: false };
			names.visit_file(&tree);
			if !names.found {
				continue;
			}
			for (at, line) in source.lines().enumerate() {
				if line.contains(name) && !line.trim_start().starts_with("//") {
					found.push(format!("{file}:{}: {name}", at + 1));
				}
			}
		}
	}
	found
}

#[test]
fn no_code_outside_the_doors_asks_for_a_human_view_or_a_private_value_for_storage() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let found = door_violations(root);
	assert!(
		found.is_empty(),
		"0681: `HumanView::for_his_eyes()` belongs only to src/frontier/today/ and src/frontier/brief/, `for_storage` only to \
		 the event queue:\n{}",
		found.join("\n")
	);
}

#[test]
fn a_door_used_in_a_file_that_may_not_names_that_file_and_line_while_the_allowed_files_pass() {
	let temp = jocasta::shared::testing::temp_dir("doors");
	let write = |file: &str, text: &str| {
		let path = temp.path().join(file);
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(path, text).unwrap();
	};
	let call = "fn show() {\n\t// HumanView::for_his_eyes() in a comment is fine.\n\tlet view = HumanView::for_his_eyes();\n}\n";
	write("src/frontier/server/tools.rs", call);
	write("src/frontier/today/view.rs", call);
	write("src/frontier/brief/model.rs", call);
	write("src/shared/private.rs", call);
	// An alias in front of it doesn't hide it.
	write("src/frontier/server/day.rs", "use x::HumanView as H;\nfn f() {\n\tlet _ = H::for_his_eyes();\n}\n");
	write("src/shared/events/queue.rs", "fn f(p: &P) {\n\tp.for_storage();\n}\n");
	write("src/frontier/server/day_view.rs", "fn f(p: &P) {\n\tp.map(P::for_storage);\n}\n");
	// A file that only mentions the name in a string or a comment never parses into a use.
	write("src/shared/log.rs", "fn f() {\n\tlet _ = \"for_his_eyes\";\n}\n");
	assert_eq!(
		door_violations(temp.path()),
		[
			"src/frontier/server/day.rs:3: for_his_eyes",
			"src/frontier/server/day_view.rs:2: for_storage",
			"src/frontier/server/tools.rs:3: for_his_eyes",
		]
	);
}

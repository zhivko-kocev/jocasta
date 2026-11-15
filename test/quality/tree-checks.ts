// The syntax-tree checks of 0610: what Biome can't see in `src`, read with the TypeScript 5 parser (0614; `tsc` 7 has
// no parser a program can call). Each finding is keyed by file, kind and qualified name, never by line, so moving code
// keeps its key.
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript5";

/** What a finding is: a boolean parameter a branch tests, a function that only forwards, an exported item without a comment. */
type TreeFindingKind = "flag-parameter" | "forwarding" | "undocumented-export";

/** `<file> <kind> <qualified name>`, the key the baseline lists. */
type TreeFinding = string;

const SOURCE = /\.ts$/;

function sourceFiles(dir: string, found: string[] = []): string[] {
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		const path = join(dir, entry.name);
		if (entry.isDirectory()) sourceFiles(path, found);
		else if (SOURCE.test(entry.name) && !entry.name.endsWith(".d.ts")) found.push(path);
	}
	return found;
}

const hasModifier = (node: ts.Node, kind: ts.SyntaxKind): boolean =>
	ts.canHaveModifiers(node) && (ts.getModifiers(node)?.some((m) => m.kind === kind) ?? false);

const isHidden = (member: ts.ClassElement): boolean =>
	hasModifier(member, ts.SyntaxKind.PrivateKeyword) ||
	hasModifier(member, ts.SyntaxKind.ProtectedKeyword) ||
	(member.name !== undefined && ts.isPrivateIdentifier(member.name));

/** Whether `node`'s text mentions the identifier `name`. */
function mentions(node: ts.Node, name: string): boolean {
	if (ts.isIdentifier(node) && node.text === name) return true;
	return ts.forEachChild(node, (child) => mentions(child, name) || undefined) ?? false;
}

/** The boolean parameters of `fn` that an `if`, a `?:` or the left of `&&`/`||` tests. */
function flagParameters(fn: ts.SignatureDeclaration & { body?: ts.Node }): string[] {
	const body = fn.body;
	if (!body) return [];
	const flags: string[] = [];
	for (const parameter of fn.parameters) {
		if (!ts.isIdentifier(parameter.name)) continue;
		const boolean =
			parameter.type?.kind === ts.SyntaxKind.BooleanKeyword ||
			parameter.initializer?.kind === ts.SyntaxKind.TrueKeyword ||
			parameter.initializer?.kind === ts.SyntaxKind.FalseKeyword;
		if (!boolean) continue;
		const name = parameter.name.text;
		const tests = (node: ts.Node): boolean => {
			if (ts.isIfStatement(node) && mentions(node.expression, name)) return true;
			if (ts.isConditionalExpression(node) && mentions(node.condition, name)) return true;
			if (
				ts.isBinaryExpression(node) &&
				(node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken ||
					node.operatorToken.kind === ts.SyntaxKind.BarBarToken) &&
				mentions(node.left, name)
			)
				return true;
			return ts.forEachChild(node, (child) => tests(child) || undefined) ?? false;
		};
		if (tests(body)) flags.push(name);
	}
	return flags;
}

/**
 * Whether `fn` only forwards: its body is one call (awaited or returned or not) whose arguments are exactly its own
 * parameters, or a method without parameters that calls `this.<path>()` with none.
 */
function forwards(fn: ts.SignatureDeclaration & { body?: ts.Node }, inClass: boolean): boolean {
	const body = fn.body;
	if (!body) return false;
	let expression: ts.Expression | undefined;
	if (!ts.isBlock(body)) expression = body as ts.Expression;
	else if (body.statements.length === 1) {
		const only = body.statements[0];
		if (only && (ts.isReturnStatement(only) || ts.isExpressionStatement(only))) expression = only.expression;
	}
	while (expression && (ts.isAwaitExpression(expression) || ts.isParenthesizedExpression(expression)))
		expression = expression.expression;
	if (!expression || !ts.isCallExpression(expression)) return false;
	const parameters = fn.parameters.filter((p) => ts.isIdentifier(p.name)).map((p) => (p.name as ts.Identifier).text);
	if (parameters.length === 0)
		return (
			inClass &&
			ts.isBlock(body) &&
			expression.arguments.length === 0 &&
			/^this\.[\w.]+$/.test(expression.expression.getText())
		);
	const args = expression.arguments.map((a) => a.getText());
	return (
		parameters.length === fn.parameters.length &&
		args.length === parameters.length &&
		args.every((a) => parameters.includes(a))
	);
}

/** The name a function-like node is known by, or undefined for an anonymous one (a callback). */
function functionName(node: ts.Node): string | undefined {
	if (ts.isConstructorDeclaration(node)) return "constructor";
	if (
		(ts.isFunctionDeclaration(node) ||
			ts.isMethodDeclaration(node) ||
			ts.isGetAccessorDeclaration(node) ||
			ts.isSetAccessorDeclaration(node)) &&
		node.name
	)
		return node.name.getText();
	const parent = node.parent;
	if ((ts.isArrowFunction(node) || ts.isFunctionExpression(node)) && parent) {
		if (ts.isVariableDeclaration(parent) && ts.isIdentifier(parent.name)) return parent.name.text;
		if (ts.isPropertyAssignment(parent) || ts.isPropertyDeclaration(parent)) return parent.name.getText();
	}
	return undefined;
}

const hasComment = (text: string, node: ts.Node): boolean =>
	(ts.getLeadingCommentRanges(text, node.getFullStart()) ?? []).length > 0;

/** The exported top-level items and the public methods of exported classes that have no comment above them. */
function undocumentedExports(file: ts.SourceFile): string[] {
	const found: string[] = [];
	for (const statement of file.statements) {
		if (!hasModifier(statement, ts.SyntaxKind.ExportKeyword)) continue;
		let name: string | undefined;
		if (
			ts.isFunctionDeclaration(statement) ||
			ts.isClassDeclaration(statement) ||
			ts.isInterfaceDeclaration(statement) ||
			ts.isTypeAliasDeclaration(statement)
		)
			name = statement.name?.text;
		else if (ts.isVariableStatement(statement)) name = statement.declarationList.declarations[0]?.name.getText(file);
		if (name === undefined) continue;
		if (!hasComment(file.text, statement)) found.push(name);
		if (!ts.isClassDeclaration(statement)) continue;
		for (const member of statement.members)
			if (ts.isMethodDeclaration(member) && !isHidden(member) && !hasComment(file.text, member))
				found.push(`${name}.${member.name.getText(file)}`);
	}
	return found;
}

/** Every finding in the `.ts` files under `dir`, keys relative to `root`, sorted. */
export function treeFindings(root: string, dir: string): TreeFinding[] {
	const findings: TreeFinding[] = [];
	for (const path of sourceFiles(dir)) {
		const at = relative(root, path).replaceAll("\\", "/");
		const file = ts.createSourceFile(path, readFileSync(path, "utf8"), ts.ScriptTarget.ESNext, true);
		const key = (kind: TreeFindingKind, name: string) => findings.push(`${at} ${kind} ${name}`);
		for (const name of undocumentedExports(file)) key("undocumented-export", name);
		const visit = (node: ts.Node, cls: string | undefined): void => {
			const inClass = ts.isClassDeclaration(node) || ts.isClassExpression(node);
			const nextClass = inClass ? (node.name?.text ?? "<class>") : cls;
			const name = ts.isFunctionLike(node) ? functionName(node) : undefined;
			if (name !== undefined) {
				const fn = node as ts.SignatureDeclaration & { body?: ts.Node };
				const qualified = cls ? `${cls}.${name}` : name;
				for (const flag of flagParameters(fn)) key("flag-parameter", `${qualified}(${flag})`);
				// An arrow given as an object's property is an adapter handed to a seam (`print: (line) => console.log(line)`),
				// not a method that only forwards.
				const adapter = node.parent !== undefined && ts.isPropertyAssignment(node.parent);
				if (!adapter && forwards(fn, cls !== undefined)) key("forwarding", qualified);
			}
			ts.forEachChild(node, (child) => visit(child, nextClass));
		};
		visit(file, undefined);
	}
	return findings.sort();
}

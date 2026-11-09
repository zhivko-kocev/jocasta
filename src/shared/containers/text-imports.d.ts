// Build-context files imported `with { type: "text" }`, so a compiled `jocasta` carries them (0074).
declare module "*Dockerfile" {
	const text: string;
	export default text;
}

declare module "*.py" {
	const text: string;
	export default text;
}

declare module "*.txt" {
	const text: string;
	export default text;
}

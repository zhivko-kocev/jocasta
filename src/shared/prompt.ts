/**
 * An appended prompt's text without its leading HTML comment (0577): Ponytail's license and file notes stay in the
 * shipped file and out of every request. Only a comment that opens the text (after blank lines) is cut, with the blank
 * lines after it; any other text is returned as it is.
 */
export function withoutLeadingComment(text: string): string {
	const start = text.match(/^\s*<!--/);
	if (!start) return text;
	const end = text.indexOf("-->", start[0].length);
	if (end === -1) return text;
	return text.slice(end + 3).replace(/^\s*\n/, "");
}

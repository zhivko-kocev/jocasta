/** A URL slug for `text`: lower case ASCII letters and digits, words joined by single dashes. */
export function slugify(text: string): string {
	return text
		.normalize("NFKD")
		.replace(/[\u0300-\u036f]/g, "")
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-");
}

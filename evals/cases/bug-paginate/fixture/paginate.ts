/** Splits `items` into pages of `size` items each. */
export function paginate<T>(items: readonly T[], size: number): T[][] {
	if (size < 1) throw new RangeError("size must be at least 1");
	const pages: T[][] = [];
	const count = Math.floor(items.length / size);
	for (let page = 0; page < count; page++) {
		pages.push(items.slice(page * size, page * size + size));
	}
	return pages;
}

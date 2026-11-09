import { expect, test } from "bun:test";
import { paginate } from "./paginate.ts";

test("full pages", () => {
	expect(paginate([1, 2, 3, 4], 2)).toEqual([
		[1, 2],
		[3, 4],
	]);
});

test("the last page may be partial", () => {
	expect(paginate([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
});

test("no items, no pages", () => {
	expect(paginate([], 3)).toEqual([]);
});

test("a size below 1 is an error", () => {
	expect(() => paginate([1], 0)).toThrow(RangeError);
});

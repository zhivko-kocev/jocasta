import { expect, test } from "bun:test";
import { memo } from "./memo.ts";

test("the same arguments run the function once", () => {
	let calls = 0;
	const square = memo((n: number) => {
		calls++;
		return n * n;
	});
	expect(square(3)).toBe(9);
	expect(square(3)).toBe(9);
	expect(calls).toBe(1);
});

test("every argument is part of the key", () => {
	const add = memo((a: number, b: number) => a + b);
	expect(add(1, 2)).toBe(3);
	expect(add(1, 3)).toBe(4);
	expect(add(1, 2)).toBe(3);
});

test("arguments that look alike are different keys", () => {
	const count = memo((...parts: string[]) => parts.length);
	expect(count("a", "b")).toBe(2);
	expect(count("a,b")).toBe(1);
	expect(count("a")).toBe(1);
});

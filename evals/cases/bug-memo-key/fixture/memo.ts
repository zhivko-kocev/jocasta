/** Wraps `fn` so that a repeated call with the same arguments returns the first call's result. */
export function memo<A extends unknown[], R>(fn: (...args: A) => R): (...args: A) => R {
	const cache = new Map<unknown, R>();
	return (...args: A): R => {
		const key = args[0];
		if (cache.has(key)) return cache.get(key) as R;
		const result = fn(...args);
		cache.set(key, result);
		return result;
	};
}

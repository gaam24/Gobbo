/// A copy of a record without one key. A computed destructuring pattern makes React Compiler skip the whole hook.
export function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
	const next = { ...record };
	delete next[key];
	return next;
}

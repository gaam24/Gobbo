/// The display name of an item by id, falling back to the id for one the list no longer knows.
export function nameOfId(items: ReadonlyArray<{ id: string; name: string }>, id: string): string {
	return items.find((item) => item.id === id)?.name ?? id;
}

/// A copy of a record without one key. A computed destructuring pattern makes React Compiler skip the whole hook.
export function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
	const next = { ...record };
	delete next[key];
	return next;
}

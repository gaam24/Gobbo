import type { LoadOrderRange } from "@/bindings";

/// The rules of the load order window. The order is a list of mod ids, first one first. The game gets a number for
/// each mod in it, counted from the lowest number its file takes; a mod outside the order gets none. Nothing here
/// decides which mod wins a conflict: that stays with the game and the list.

export interface OrderableMod {
	id: string;
	load_priority?: number | null;
}

/// The order a profile already has: the mods with a number, lowest number first.
export function orderFromMods(mods: readonly OrderableMod[]): string[] {
	return mods
		.filter((mod): mod is OrderableMod & { load_priority: number } => typeof mod.load_priority === "number")
		.toSorted((left, right) => left.load_priority - right.load_priority)
		.map((mod) => mod.id);
}

/// The mods the window can number: the ones whose folder the game reads in order. Any other mod would get a number the
/// game never reads, so the window does not list it.
export function orderableMods<T extends { orderable: boolean }>(mods: readonly T[]): T[] {
	return mods.filter((mod) => mod.orderable);
}

/// The mods the window offers to add: the orderable ones that are not in the order yet, in the list's order.
export function availableMods<T extends { id: string; orderable: boolean }>(mods: readonly T[], order: readonly string[]): T[] {
	const inOrder = new Set(order);
	return mods.filter((mod) => mod.orderable && !inOrder.has(mod.id));
}

/// The mods that keep a number the game does not read, because their folder is not read in order (a layout change can
/// cause that). The window leaves them out of the order; saving the order clears their numbers.
export function unorderedNumbers<T extends OrderableMod & { orderable: boolean }>(mods: readonly T[]): T[] {
	return mods.filter((mod) => !mod.orderable && typeof mod.load_priority === "number");
}

/// Whether no mod of the profile can take a number, so the window says why instead of listing nothing.
export function nothingOrderable(mods: ReadonlyArray<{ orderable: boolean }>): boolean {
	return !mods.some((mod) => mod.orderable);
}

/// The order with the mod at `index` moved one place. A move past either end leaves the order as it is.
export function moveBy(order: readonly string[], index: number, step: -1 | 1): string[] {
	const target = index + step;
	const next = [...order];
	const current = next[index];
	const other = next[target];
	if (current === undefined || other === undefined) {
		return next;
	}
	next[index] = other;
	next[target] = current;
	return next;
}

/// The order without a mod.
export function withoutId(order: readonly string[], id: string): string[] {
	return order.filter((value) => value !== id);
}

/// The order with a mod added at the end. A mod already in it stays where it is.
export function withAppended(order: readonly string[], id: string): string[] {
	return order.includes(id) ? [...order] : [...order, id];
}

/// How many mods the game's range can number.
export function capacity(range: LoadOrderRange): number {
	return range.priority_max - range.priority_min + 1;
}

/// The number the game gets for the mod at `index`.
export function numberAt(range: LoadOrderRange, index: number): number {
	return range.priority_min + index;
}

/// Whether the order has no more mods than the range has numbers.
export function fits(order: readonly string[], range: LoadOrderRange): boolean {
	return order.length <= capacity(range);
}

/// Whether two orders list the same mods in the same places.
export function sameOrder(left: readonly string[], right: readonly string[]): boolean {
	return left.length === right.length && left.every((id, index) => id === right[index]);
}

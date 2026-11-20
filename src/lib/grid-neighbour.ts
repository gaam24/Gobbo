/// A card of the grid: its id and its box on the screen.
export interface GridCell {
	id: string;
	top: number;
	left: number;
	right: number;
}

/// Cards whose tops differ by less than this are in the same row (sub-pixel rounding of the layout).
const ROW_TOLERANCE = 2;

/// The card above or below `id` on the screen: the nearest row in that direction, and the card of that row whose centre
/// is closest to the centre of `id`. `null` when there is no such card, or when `id` is not on the screen.
export function gridNeighbour(cells: readonly GridCell[], id: string, direction: "up" | "down"): string | null {
	const from = cells.find((cell) => cell.id === id);

	if (!from) return null;
	const above = direction === "up";
	const candidates = cells.filter((cell) => (above ? cell.top < from.top - ROW_TOLERANCE : cell.top > from.top + ROW_TOLERANCE));

	if (candidates.length === 0) return null;
	const edge = candidates.reduce((best, cell) => (above ? Math.max(best, cell.top) : Math.min(best, cell.top)), candidates[0]?.top ?? 0);
	const row = candidates.filter((cell) => Math.abs(cell.top - edge) <= ROW_TOLERANCE);
	const centre = (cell: GridCell) => (cell.left + cell.right) / 2;
	const target = centre(from);
	let best: GridCell | undefined;

	for (const cell of row) {
		if (!best || Math.abs(centre(cell) - target) < Math.abs(centre(best) - target)) best = cell;
	}

	return best?.id ?? null;
}

import { ModGrid } from "@/views/mod-grid/mod-grid";
import type { ModListProps } from "@/views/mod-table/mod-list-props";
import { ModTable } from "@/views/mod-table/mod-table";
import type { ModView } from "@/views/mods/use-mod-view";

/// The mod list in the chosen view: the table, or the grid of cards. Both take the same props, so switching loses nothing.
export function ModList({ view, ...props }: ModListProps & { view: ModView }) {
	return view === "grid" ? <ModGrid {...props} /> : <ModTable {...props} />;
}

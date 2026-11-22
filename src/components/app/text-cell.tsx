/// The dash a cell shows when the value is missing. Kept here so the shared cell does not depend on the mod table.
const EMPTY = "—";

/// Text that may be missing. The cell truncates long text, so the full text is in the title.
export function TextCell({ value }: { value: string | null | undefined }) {
	if (!value) return <span className="text-subtle-foreground text-xs">{EMPTY}</span>;
	return (
		<span className="text-muted-foreground block truncate text-xs" title={value}>
			{value}
		</span>
	);
}

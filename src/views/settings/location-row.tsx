import { Item, ItemActions, ItemContent } from "@/components/ui/item";
import { PathActions } from "@/views/settings/path-actions";

/// One path with buttons to open it in the file manager and to copy it to the clipboard.
export function LocationRow({
	label,
	path,
	emptyHint,
	onOpen,
	onCopy,
}: {
	label: string;
	path: string;
	emptyHint?: string;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
}) {
	return (
		<Item asChild variant="outline" size="sm" className="min-w-0">
			<li>
				<ItemContent className="min-w-0 gap-0.5 md:flex-row md:items-center md:gap-4">
					<span className="text-sm md:w-40 md:shrink-0">{label}</span>
					<span className="text-muted-foreground min-w-0 flex-1 truncate font-mono text-xs" title={path || undefined}>
						{path || emptyHint}
					</span>
				</ItemContent>
				<ItemActions>
					<PathActions path={path} onOpen={onOpen} onCopy={onCopy} />
				</ItemActions>
			</li>
		</Item>
	);
}

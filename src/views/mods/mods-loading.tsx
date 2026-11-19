import { Skeleton } from "@/components/ui/skeleton";
import { useTranslation } from "@/i18n";

/// The widths of the loading rows' names, so they do not look like one block.
const SKELETON_ROWS = ["14rem", "9rem", "18rem", "11rem", "15rem", "8rem"];

/// The list while the game's status loads: the shape of the page, so nothing jumps when it comes.
export function ModsLoading() {
	const { t } = useTranslation();
	return (
		<div role="status" aria-label={t("mods.loading")} className="flex min-h-0 flex-1 flex-col">
			<div className="flex h-12 items-center gap-3 border-b px-6">
				<Skeleton className="h-4 w-40" />
				<Skeleton className="ml-auto h-8 w-36 rounded-md" />
			</div>
			<div className="flex h-14 items-center gap-3 border-b px-6">
				<Skeleton className="h-9 w-56 rounded-md" />
				<Skeleton className="h-8 w-80 rounded-md" />
			</div>
			<div className="grid gap-px">
				{SKELETON_ROWS.map((width) => (
					<div key={width} className="flex h-14 items-center gap-4 border-b px-6">
						<Skeleton className="size-4" />
						<Skeleton className="h-5 w-9 rounded-full" />
						<Skeleton className="h-4" style={{ width }} />
					</div>
				))}
			</div>
		</div>
	);
}

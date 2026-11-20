import { cn } from "cn";
import type { HistoryEntry_Serialize } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Item, ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { formatHistoryTime, summaryParts } from "@/lib/history";

/// The kept states of the active profile, newest first. Each row says when the change happened and what it did;
/// selecting a row shows what restoring that state would change.
export function HistoryTimeline({
	entries,
	selectedId,
	onSelect,
	total,
	enabled,
	locale,
}: {
	entries: HistoryEntry_Serialize[];
	selectedId: number | null;
	onSelect: (id: number) => void;
	/// The current list, shown above the kept states.
	total: number;
	enabled: number;
	locale: string;
}) {
	const { t } = useTranslation();
	/// Entries saved before change summaries existed. They are explained once, under the list, not on every row.
	const hasUnsummarized = entries.some((entry) => entry.summary === null);

	/// The entries keep the listbox role (one is selected) and take the look of a `BoxList`.
	return (
		<div className="grid content-start gap-2">
			<Item variant="muted" size="sm" className="rounded-lg border">
				<ItemContent className="gap-0.5">
					<ItemTitle>{t("history.now")}</ItemTitle>
					<span className="text-muted-foreground text-xs tabular-nums" title={t("history.stateNow")}>
						{t("history.state", { enabled, total })}
					</span>
				</ItemContent>
			</Item>
			{entries.length > 0 && (
				<div role="listbox" aria-label={t("history.title")} className="rounded-lg border">
					{entries.map((entry) => {
						const parts = summaryParts(entry.summary);
						const selected = entry.id === selectedId;
						return (
							<Item
								key={entry.id}
								role="option"
								aria-selected={selected}
								tabIndex={0}
								size="sm"
								className={cn(
									"border-border cursor-pointer rounded-none border-0 border-b px-3 py-2 text-left last:border-b-0",
									selected ? "bg-accent/60" : "hover:bg-muted/60",
								)}
								onClick={() => onSelect(entry.id)}
								onKeyDown={(event) => {
									if (event.key === "Enter" || event.key === " ") {
										event.preventDefault();
										onSelect(entry.id);
									}
								}}
							>
								<ItemContent className="gap-1">
									<div className="flex flex-wrap items-baseline justify-between gap-2">
										<ItemTitle className="tabular-nums">{formatHistoryTime(entry.at, locale)}</ItemTitle>
										<span className="text-muted-foreground text-xs tabular-nums" title={t("history.stateBefore")}>
											{t("history.state", {
												enabled: entry.enabled,
												total: entry.total,
											})}
										</span>
									</div>
									<div className="flex flex-wrap gap-1">
										{parts.map((part) => (
											<Badge key={part.kind} variant="secondary">
												{t(`history.change.${part.kind}`, {
													count: part.count,
												})}
											</Badge>
										))}
										{entry.summary !== null && parts.length === 0 && (
											<span className="text-muted-foreground text-xs">{t("history.changed")}</span>
										)}
									</div>
								</ItemContent>
							</Item>
						);
					})}
				</div>
			)}
			{hasUnsummarized && <p className="text-muted-foreground text-xs">{t("history.unsummarized")}</p>}
		</div>
	);
}

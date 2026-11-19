import { Search } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { useSettingsData } from "@/app/contexts/settings-context";
import type { CollectionItem_Serialize, CollectionItemStatus } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { StatusBadge } from "@/components/app/status-badge";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Empty, EmptyDescription } from "@/components/ui/empty";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useTranslation } from "@/i18n";
import {
	filterCounts,
	MEMBER_FILTERS,
	type MemberFilter,
	matchesFilter,
	matchesSearch,
	providerName,
	sourceLabelKey,
	statusLabelKey,
	statusTone,
	statusVersion,
} from "@/lib/collections";
import { formatSize } from "@/lib/mods";

const FILTER_LABELS = {
	all: "collection.filter.all",
	todo: "collection.filter.todo",
	done: "collection.filter.done",
	optional: "collection.filter.optional",
	issues: "collection.filter.issues",
} as const satisfies Record<MemberFilter, string>;

/// The members of one collection: a search, the filters with their counts, and one row per member. The preview passes
/// a selection to show checkboxes; the panel and the install view pass a live state or an action per row.
export function CollectionMemberList({
	items,
	statuses,
	provider,
	selected,
	onToggle,
	renderState,
	renderAction,
	className,
}: {
	items: readonly CollectionItem_Serialize[];
	statuses: Partial<Record<string, CollectionItemStatus>>;
	/// The collection's own provider. A member from it shows no source; only a member from elsewhere names one.
	provider: string;
	selected?: ReadonlySet<string>;
	onToggle?: (key: string, checked: boolean) => void;
	/// A live label that replaces the status, for example "Downloading" while the automatic install runs.
	renderState?: (item: CollectionItem_Serialize) => string | null;
	renderAction?: (item: CollectionItem_Serialize) => ReactNode;
	className?: string;
}) {
	const { t, i18n } = useTranslation();
	const { providers } = useSettingsData();
	const [filter, setFilter] = useState<MemberFilter>("all");
	const [query, setQuery] = useState("");
	const counts = useMemo(() => filterCounts(items, statuses), [items, statuses]);
	const shown = items.filter((item) => matchesFilter(item, statuses[item.key], filter) && matchesSearch(item, query));
	const sourceText = (item: CollectionItem_Serialize): string | null => {
		if (item.source.kind === "provider") {
			return item.source.provider === provider ? null : providerName(providers, item.source.provider);
		}
		const key = sourceLabelKey(item.source);
		return key ? t(key) : null;
	};
	const filters = MEMBER_FILTERS.filter((value) => value === "all" || counts[value] > 0);

	return (
		<div className={`grid min-h-0 gap-2 ${className ?? ""}`}>
			<div className="flex flex-wrap items-center gap-2">
				<ToggleGroup
					type="single"
					value={filter}
					onValueChange={(value) => setFilter(MEMBER_FILTERS.find((known) => known === value) ?? "all")}
					spacing={0}
					aria-label={t("collection.filter.label")}
					className="bg-muted max-w-full overflow-x-auto rounded-sm p-0.5"
				>
					{filters.map((value) => (
						<ToggleGroupItem
							key={value}
							value={value}
							className="data-[state=on]:bg-background h-7 shrink-0 gap-1 px-2 data-[state=on]:font-semibold"
						>
							{t(FILTER_LABELS[value])}
							<span className="text-muted-foreground font-mono text-xs tabular-nums">{counts[value]}</span>
						</ToggleGroupItem>
					))}
				</ToggleGroup>
				<InputGroup className="h-8 min-w-40 flex-1">
					<InputGroupAddon>
						<Search />
					</InputGroupAddon>
					<InputGroupInput
						value={query}
						placeholder={t("collection.filter.search")}
						aria-label={t("collection.filter.search")}
						onChange={(event) => setQuery(event.currentTarget.value)}
					/>
				</InputGroup>
			</div>
			{shown.length === 0 ? (
				<div className="min-h-0 overflow-auto rounded-lg border">
					<Empty className="py-8">
						<EmptyDescription>{t("collection.filter.empty")}</EmptyDescription>
					</Empty>
				</div>
			) : (
				<BoxList className="min-h-0 overflow-auto">
					{shown.map((item) => {
						const status = statuses[item.key];
						const live = renderState?.(item) ?? null;
						const source = sourceText(item);
						const id = `collection-member-${item.key}`;
						const checked = selected?.has(item.key) ?? false;
						return (
							<BoxListRow key={item.key} className="flex-nowrap gap-3">
								{selected && onToggle && (
									<Checkbox id={id} checked={checked} onCheckedChange={(value) => onToggle(item.key, value === true)} />
								)}
								<div className="grid min-w-0 flex-1 gap-0.5">
									<div className="flex min-w-0 items-center gap-2">
										<label
											htmlFor={selected ? id : undefined}
											className={`min-w-0 truncate font-medium ${selected ? "cursor-pointer" : ""} ${selected && !checked ? "text-muted-foreground" : ""}`}
											title={item.name}
										>
											{item.name}
										</label>
										{item.version && <span className="text-subtle-foreground shrink-0 font-mono text-xs">{item.version}</span>}
										{item.optional && (
											<Badge variant="outline" className="shrink-0">
												{t("collection.optional")}
											</Badge>
										)}
									</div>
									{(item.author ?? source) && (
										<span className="text-muted-foreground truncate text-xs">{[item.author, source].filter(Boolean).join(" · ")}</span>
									)}
								</div>
								{live ? (
									<StatusBadge tone="info" dot className="shrink-0">
										{live}
									</StatusBadge>
								) : (
									<StatusBadge tone={statusTone(status)} dot className="shrink-0">
										{t(statusLabelKey(status), { version: statusVersion(status) })}
									</StatusBadge>
								)}
								<span className="text-subtle-foreground w-16 shrink-0 text-right font-mono text-xs tabular-nums">
									{item.size_kb > 0 ? formatSize(item.size_kb, i18n.language) : "—"}
								</span>
								{renderAction?.(item)}
							</BoxListRow>
						);
					})}
				</BoxList>
			)}
		</div>
	);
}

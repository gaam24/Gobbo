import { cn } from "cn";
import { ChevronDown, Search, Tag, X } from "lucide-react";
import type { Dispatch, RefObject, SetStateAction } from "react";
import { Button } from "@/components/ui/button";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useTranslation } from "@/i18n";
import type { Filter } from "@/lib/mods";

const FILTERS = [
	{ value: "all", label: "mods.filter.all" },
	{ value: "enabled", label: "mods.filter.enabled" },
	{ value: "disabled", label: "mods.filter.disabled" },
	{ value: "missing", label: "mods.filter.missing" },
	{ value: "conflict", label: "mods.filter.conflict" },
	{ value: "updates", label: "mods.filter.updates" },
	{ value: "hidden", label: "mods.filter.hidden" },
] as const satisfies ReadonlyArray<{ value: Filter; label: string }>;

/// The state of the filters above the mod list, owned by `useModFilter`.
export interface FilterBarProps {
	searchRef: RefObject<HTMLInputElement | null>;
	filter: Filter;
	setFilter: Dispatch<SetStateAction<Filter>>;
	query: string;
	setQuery: Dispatch<SetStateAction<string>>;
	tagFilter: string | null;
	setTagFilter: Dispatch<SetStateAction<string | null>>;
	allTags: string[];
	counts: Record<Filter, number>;
	/// A search, a status or a tag narrows the list. The toolbar's list actions (sorting, separators) are off then.
	filtering: boolean;
}

/// The search, the status filter and the tag filter above the mod list. The parts sit in the toolbar's flex row.
export function FilterBar({ searchRef, filter, setFilter, query, setQuery, tagFilter, setTagFilter, allTags, counts }: FilterBarProps) {
	const { t } = useTranslation();
	/// The Hidden filter appears only once the list hides a mod, or while it is the filter in use.
	const shownFilters = FILTERS.filter(({ value }) => value !== "hidden" || counts.hidden > 0 || filter === "hidden");
	const changeFilter = (value: string) => {
		const next = FILTERS.find((item) => item.value === value);
		if (next) setFilter(next.value);
	};

	return (
		<>
			<InputGroup className="min-w-36 flex-1 basis-36 @3xl:max-w-64">
				<InputGroupInput
					ref={searchRef}
					aria-label={t("mods.search")}
					placeholder={t("mods.searchHint")}
					value={query}
					onChange={(event) => setQuery(event.currentTarget.value)}
					onKeyDown={(event) => {
						// Escape clears a search first, and leaves the field when there is nothing to clear.
						if (event.key !== "Escape") return;
						if (query) setQuery("");
						else event.currentTarget.blur();
						event.stopPropagation();
					}}
				/>
				<InputGroupAddon>
					<Search aria-hidden />
				</InputGroupAddon>
				{query && (
					<InputGroupAddon align="inline-end">
						<InputGroupButton
							size="icon-xs"
							aria-label={t("mods.clearSearch")}
							title={t("mods.clearSearch")}
							onClick={() => {
								setQuery("");
								searchRef.current?.focus();
							}}
						>
							<X aria-hidden />
						</InputGroupButton>
					</InputGroupAddon>
				)}
			</InputGroup>
			<ToggleGroup
				type="single"
				value={filter}
				onValueChange={changeFilter}
				spacing={0}
				aria-label={t("mods.filter.label")}
				className="bg-muted max-w-full overflow-x-auto rounded-sm p-0.5"
			>
				{shownFilters.map(({ value, label }) => (
					<ToggleGroupItem
						key={value}
						value={value}
						className="text-muted-foreground data-[state=on]:bg-secondary data-[state=on]:text-foreground h-7 shrink-0 gap-1 px-2 data-[state=on]:font-semibold"
					>
						{t(label)}
						<span className="text-muted-foreground font-mono text-xs tabular-nums">{counts[value]}</span>
					</ToggleGroupItem>
				))}
			</ToggleGroup>
			{allTags.length > 0 && (
				<Popover>
					<PopoverTrigger asChild>
						<Button variant={tagFilter ? "secondary" : "ghost"} aria-label={t("mods.tagFilter.button")}>
							<Tag /> {tagFilter ?? `# ${t("mods.tagFilter.button")}`} <ChevronDown />
						</Button>
					</PopoverTrigger>
					<PopoverContent className="w-56 p-2" align="start">
						<div role="listbox" aria-label={t("mods.tagFilter.label")} className="grid gap-1">
							{allTags.map((tag) => (
								<Button
									key={tag}
									variant="ghost"
									size="sm"
									role="option"
									aria-selected={tagFilter === tag}
									className={cn("justify-start", tagFilter === tag && "bg-secondary font-semibold")}
									onClick={() => setTagFilter((prev) => (prev === tag ? null : tag))}
								>
									{tag}
								</Button>
							))}
							{tagFilter && (
								<Button variant="ghost" size="sm" className="text-muted-foreground mt-1 justify-start" onClick={() => setTagFilter(null)}>
									{t("mods.tagFilter.clear")}
								</Button>
							)}
						</div>
					</PopoverContent>
				</Popover>
			)}
		</>
	);
}

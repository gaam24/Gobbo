import type {
	CollectionBadge,
	CollectionItem_Serialize,
	CollectionItemStatus,
	CollectionOrigin_Serialize,
	CollectionPlan_Serialize,
	CollectionSource_Serialize,
	SourceProviderInfo,
} from "@/bindings";
import type { StatusTone } from "@/components/app/status-badge";
import type { TranslationKey } from "@/i18n";
import { foldText } from "@/lib/fold-text";

/// The key the IPC and the profile badge use for one collection.
export function collectionKey(origin: Pick<CollectionOrigin_Serialize, "provider" | "game_key" | "slug">): string {
	return `${origin.provider}:${origin.game_key}:${origin.slug}`;
}

/// The revision as the provider names it (`r49`, `v1.2.3`). An origin saved before labels existed shows `r<number>`.
export function revisionLabel(value: { revision: number; revision_label?: string | null }): string {
	const label = value.revision_label?.trim() ?? "";
	return label.length > 0 ? label : `r${value.revision}`;
}

/// Whether any collection of the profile has a newer revision to review.
export function hasCollectionUpdate(collections: CollectionBadge[]): boolean {
	return collections.some((item) => item.update);
}

/// The required members are selected for a first install; an optional member starts unchecked.
export function defaultSelection(plan: CollectionPlan_Serialize): string[] {
	return plan.items.filter((item) => !item.optional).map((item) => item.key);
}

/// Whether a status already satisfies the member.
export function isDone(status: CollectionItemStatus | undefined): boolean {
	return status?.kind === "installed" || status?.kind === "satisfied" || status?.kind === "manual" || status?.kind === "skipped";
}

/// Whether the member still needs a download, an import or the user.
export function needsWork(status: CollectionItemStatus | undefined): boolean {
	return !isDone(status) && status?.kind !== "unsupported";
}

/// The members in the order they enter the list. Keys the order does not name come last, so a damaged plan still lists
/// every member.
export function orderedItems(plan: CollectionPlan_Serialize): CollectionItem_Serialize[] {
	const byKey = new Map(plan.items.map((item) => [item.key, item]));
	const ordered = plan.order.flatMap((key) => {
		const item = byKey.get(key);

		if (!item) return [];
		byKey.delete(key);
		return [item];
	});
	return [...ordered, ...byKey.values()];
}

/// The label key of a member's source. A provider member is labelled with the provider's own name instead.
export function sourceLabelKey(source: CollectionSource_Serialize): TranslationKey | null {
	switch (source.kind) {
		case "provider":
			return null;
		case "bundle":
			return "collection.source.bundle";
		case "direct":
			return "collection.source.direct";
		case "browse":
			return "collection.source.browse";
		case "manual":
			return "collection.source.manual";
		case "unsupported":
			return "collection.source.unsupported";
		default:
			return "collection.source.unsupported";
	}
}

/// The display name of a provider id, from the providers the backend reports. An unknown id shows as it is.
export function providerName(providers: readonly SourceProviderInfo[], id: string): string {
	return providers.find((provider) => provider.id === id)?.name ?? id;
}

const STATUS_LABELS: Record<CollectionItemStatus["kind"], TranslationKey> = {
	missing: "collection.status.missing",
	installed: "collection.status.installed",
	satisfied: "collection.status.satisfied",
	queued: "collection.status.queued",
	in_library: "collection.status.inLibrary",
	different_version: "collection.status.differentVersion",
	in_library_different_version: "collection.status.inLibraryDifferentVersion",
	skipped: "collection.status.skipped",
	manual: "collection.status.manual",
	unsupported: "collection.status.unsupported",
};

const STATUS_TONES: Record<CollectionItemStatus["kind"], StatusTone> = {
	missing: "neutral",
	installed: "success",
	satisfied: "warning",
	queued: "info",
	in_library: "info",
	different_version: "warning",
	in_library_different_version: "warning",
	skipped: "neutral",
	manual: "success",
	unsupported: "danger",
};

/// The label key of one member's state.
export function statusLabelKey(status: CollectionItemStatus | undefined): TranslationKey {
	return status ? STATUS_LABELS[status.kind] : "collection.status.missing";
}

/// The badge tone of one member's state. The label always says the same thing in words.
export function statusTone(status: CollectionItemStatus | undefined): StatusTone {
	return status ? STATUS_TONES[status.kind] : "neutral";
}

/// The version a member's state label names: the one installed or in the library, when the state has one.
export function statusVersion(status: CollectionItemStatus | undefined): string | undefined {
	if (status?.kind === "satisfied" || status?.kind === "different_version" || status?.kind === "in_library_different_version") {
		return status.current;
	}

	return undefined;
}

/// The filters of a member list.
export type MemberFilter = "all" | "todo" | "done" | "optional" | "issues";

export const MEMBER_FILTERS: readonly MemberFilter[] = ["all", "todo", "done", "optional", "issues"];

/// Whether a member shows under `filter`.
export function matchesFilter(item: CollectionItem_Serialize, status: CollectionItemStatus | undefined, filter: MemberFilter): boolean {
	switch (filter) {
		case "all":
			return true;
		case "todo":
			return needsWork(status);
		case "done":
			return isDone(status) && status?.kind !== "skipped";
		case "optional":
			return item.optional;
		case "issues":
			return status?.kind === "unsupported" || status?.kind === "different_version" || status?.kind === "in_library_different_version";
		default:
			return true;
	}
}

/// How many members each filter shows. The filter buttons always show these counts.
export function filterCounts(
	items: readonly CollectionItem_Serialize[],
	statuses: Partial<Record<string, CollectionItemStatus>>,
): Record<MemberFilter, number> {
	const counts: Record<MemberFilter, number> = {
		all: 0,
		todo: 0,
		done: 0,
		optional: 0,
		issues: 0,
	};

	for (const item of items) {
		for (const filter of MEMBER_FILTERS) {
			if (matchesFilter(item, statuses[item.key], filter)) counts[filter] += 1;
		}
	}

	return counts;
}

/// Whether a member matches the search text: its name, author or version.
export function matchesSearch(item: CollectionItem_Serialize, query: string): boolean {
	const text = foldText(query.trim());

	if (!text) return true;
	return [item.name, item.author ?? "", item.version ?? ""].some((value) => foldText(value).includes(text));
}

/// The download size in KB of the selected members that still need a download.
export function downloadSizeKb(
	items: readonly CollectionItem_Serialize[],
	statuses: Partial<Record<string, CollectionItemStatus>>,
	selected: ReadonlySet<string>,
): number {
	return items
		.filter((item) => selected.has(item.key))
		.filter((item) => {
			const kind = statuses[item.key]?.kind;
			return kind === undefined || kind === "missing" || kind === "queued" || kind === "different_version";
		})
		.reduce((total, item) => total + item.size_kb, 0);
}

/// A member's name with its version. Two files of one page are two members; the version tells them apart while the
/// window waits for the download the user has to start.
export function collectionItemLabel(items: readonly CollectionItem_Serialize[], key: string): string {
	const item = items.find((entry) => entry.key === key);

	if (!item) return key;
	return [item.name, item.version].filter(Boolean).join(" ");
}

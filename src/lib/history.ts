import type { HistoryPreview_Serialize, HistorySummary } from "@/bindings";
import { formatDateTime, formatTime, isToday } from "@/lib/mods";

/// The parts of a change, in the order the timeline shows them. A zero count is left out of the badges.
export const HISTORY_PARTS = [
	"added",
	"removed",
	"updated",
	"moved",
	"enabled_on",
	"enabled_off",
	"priority",
	"groups",
	"winners",
	"waiting",
] as const;

export type HistoryPart = (typeof HISTORY_PARTS)[number];

export interface HistoryPartCount {
	kind: HistoryPart;
	count: number;
}

/// The non-zero parts of a summary. A missing summary (an entry written before summaries existed) has no parts.
export function summaryParts(summary: HistorySummary | null | undefined): HistoryPartCount[] {
	if (!summary) return [];
	return HISTORY_PARTS.map((kind) => ({ kind, count: summary[kind] })).filter((part) => part.count > 0);
}

/// Whether restoring the preview changes anything. A state identical to the current list can be read but not restored.
export function hasPreviewChanges(preview: HistoryPreview_Serialize): boolean {
	return (
		preview.adds.length > 0 ||
		preview.removes.length > 0 ||
		preview.versions.length > 0 ||
		preview.switches_on.length > 0 ||
		preview.switches_off.length > 0 ||
		preview.moved > 0 ||
		preview.priority > 0 ||
		preview.groups > 0 ||
		preview.winners > 0 ||
		preview.waiting > 0
	);
}

/// The time of a change: the clock time for today, the date and time for older entries.
export function formatHistoryTime(seconds: number, locale: string): string {
	const ms = seconds * 1000;
	return isToday(ms) ? formatTime(ms, locale) : formatDateTime(ms, locale);
}

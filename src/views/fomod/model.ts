import type { GroupKind, OptionKind } from "@/bindings";
import type { FomodChoice } from "@/lib/fomod";

/// Where a step sits in the walk through the installer: behind, current, or ahead.
export function stepState(index: number, current: number): "fomod.step.done" | "fomod.step.current" | "fomod.step.upcoming" {
	if (index < current) return "fomod.step.done";
	if (index === current) return "fomod.step.current";
	return "fomod.step.upcoming";
}

export const GROUP_HINT = {
	select_any: "fomod.group.selectAny",
	select_at_least_one: "fomod.group.selectAtLeastOne",
	select_exactly_one: "fomod.group.selectExactlyOne",
	select_at_most_one: "fomod.group.selectAtMostOne",
	select_all: "fomod.group.selectAll",
} as const satisfies Record<GroupKind, string>;

/// The pill next to an option name. `could_be_usable` options depend on other choices, so they say so.
export const OPTION_TAG: Partial<Record<OptionKind, "fomod.option.required" | "fomod.option.recommended" | "fomod.option.conditional">> = {
	required: "fomod.option.required",
	recommended: "fomod.option.recommended",
	could_be_usable: "fomod.option.conditional",
};

/// Presentation only: a group that takes one option is drawn with radio buttons, the others with checkboxes. The
/// answer to a click is decided by the backend (`fomod_toggle_pick`), never by this role.
export function isRadio(kind: GroupKind): boolean {
	return kind === "select_exactly_one" || kind === "select_at_most_one";
}

/// Presentation only: a group whose listed options are all required cannot be clicked, so its hint says so. The kinds
/// are the effective ones from the backend answer, including a `<dependencyType>` that makes an option required.
export function isLocked(kinds: readonly OptionKind[]): boolean {
	return kinds.length > 0 && kinds.every((kind) => kind === "required");
}

/// The key of one group in the wizard: its step and its place in that step. Group names may repeat, so the position is
/// the identity.
export function groupKey(step: number, group: number): string {
	return `${step}:${group}`;
}

/// A stable text of a selection, independent of the order its choices were made in. Two selections with the same
/// answers have the same key, so a repeated answer is not asked again.
export function selectionKey(selection: readonly FomodChoice[]): string {
	return selection
		.map((choice) => `${choice.step ?? ""}:${choice.group_index ?? ""}:${choice.option}`)
		.toSorted()
		.join("\n");
}

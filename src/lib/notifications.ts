import type { NOTIFICATION_OUTCOMES, TASK_KINDS } from "@/bindings";

export type TaskKind = (typeof TASK_KINDS)[number];
export type NotificationOutcome = (typeof NOTIFICATION_OUTCOMES)[number];

/// The stable code of one task notification, the same `<task kind>.<outcome>` the backend's `task_notification` builds.
/// The settings store these codes in `muted_notifications`.
export function taskNotification(kind: TaskKind, outcome: NotificationOutcome): string {
	return `${kind}.${outcome}`;
}

/// The saved list with one code muted or unmuted. A code this version does not know is kept, so the choices of a newer
/// version survive.
export function withMuted(muted: readonly string[], code: string, on: boolean): string[] {
	const rest = muted.filter((item) => item !== code);
	return on ? rest : [...rest, code];
}

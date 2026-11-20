/// A button kept with a notice for a result the user may still want to act on later, such as Undo. The callback lives
/// in memory only: a reload keeps the notice text but drops the button, since a function cannot be persisted.
export interface NoticeAction {
	label: string;
	onClick: () => void;
}

/// One entry of the notification list. The rail badge counts the unread ones; the notifications page shows them all.
export interface Notice {
	id: string;
	kind: "info" | "error";
	text: string;
	/// The technical side of an error, such as the backend's error code. Missing when it adds nothing to the text.
	detail?: string;
	/// When the notice came, in milliseconds since the epoch.
	at: number;
	/// Whether the user has seen it. Opening the notifications page reads every notice.
	read: boolean;
	/// A recovery action that outlives the toast. Set only by `notifyDurable`; ordinary notices leave it out.
	action?: NoticeAction;
}

/// The most notices the list keeps. The oldest go first, so a long session cannot grow the list without limit.
export const MAX_NOTICES = 200;

/// Each start has its own prefix for the ids, so a notice kept from an earlier start never shares an id with a new one.
const SESSION = Date.now().toString(36);
let noticeCount = 0;

/// The id tells a dismiss which notice to remove. Detail is kept only when it says more than the text; an action is
/// kept only by the durable path that passes one.
export function newNotice(kind: Notice["kind"], text: string, detail?: string, action?: NoticeAction): Notice {
	noticeCount += 1;
	const notice: Notice = {
		id: `${SESSION}-${noticeCount}`,
		kind,
		text,
		at: Date.now(),
		read: false,
	};
	const withDetail = detail && detail !== text ? { ...notice, detail } : notice;
	return action ? { ...withDetail, action } : withDetail;
}

export function addNotice(list: Notice[], notice: Notice): Notice[] {
	return [notice, ...list].slice(0, MAX_NOTICES);
}

export function readAll(list: Notice[]): Notice[] {
	return list.some((notice) => !notice.read) ? list.map((notice) => (notice.read ? notice : { ...notice, read: true })) : list;
}

/// How many notices are unread, and whether one of those is an error: the bell shows an error in red.
export function unreadSummary(notices: Notice[]) {
	const unread = notices.filter((notice) => !notice.read);
	return {
		count: unread.length,
		error: unread.some((notice) => notice.kind === "error"),
	};
}

/// Notices that say the same thing one after another, shown as one row. `notice` is the newest of them; `ids` lists all of
/// them, so one dismiss removes the whole row.
export interface NoticeGroup {
	notice: Notice;
	count: number;
	ids: string[];
}

/// The list in order, with each run of identical notices merged into one group. A notice joins the group above only when
/// its kind, text and detail match, so a message that comes back later still shows where it belongs in the list.
export function groupNotices(notices: readonly Notice[]): NoticeGroup[] {
	const groups: NoticeGroup[] = [];

	for (const notice of notices) {
		const last = groups.at(-1);

		if (last && last.notice.kind === notice.kind && last.notice.text === notice.text && last.notice.detail === notice.detail) {
			last.count += 1;
			last.ids.push(notice.id);
		} else {
			groups.push({ notice, count: 1, ids: [notice.id] });
		}
	}

	return groups;
}

/// The toast-side twin of `groupNotices`: consecutive results that say the same thing fold into one toast. The count
/// drives the "+N more" suffix, so a burst of identical messages cannot fill the corner with the same card.
export interface ToastRun {
	signature: string;
	count: number;
}

export function continueToastRun(run: ToastRun | null, signature: string): { run: ToastRun; repeated: boolean } {
	if (run?.signature === signature) {
		return { run: { signature, count: run.count + 1 }, repeated: true };
	}

	return { run: { signature, count: 1 }, repeated: false };
}

/// Dismisses every notice of a group. Each dismiss is a functional update on the list, so the removals never overwrite
/// each other.
export function dismissGroup(group: NoticeGroup, dismiss: (id: string) => void): void {
	for (const id of group.ids) dismiss(id);
}

/// A stored action has its label; JSON drops the `onClick` function, so the notifications list hides the button after
/// a reload. Accept a label-only action, but reject one whose label is not text.
function isNoticeAction(value: unknown): value is NoticeAction {
	return (
		typeof value === "object" &&
		value !== null &&
		"label" in value &&
		typeof value.label === "string" &&
		(!("onClick" in value) || typeof value.onClick === "function")
	);
}

/// A notice read back from storage: every field has its type, and only `detail` and `action` may be missing.
function isNotice(value: unknown): value is Notice {
	return (
		typeof value === "object" &&
		value !== null &&
		"id" in value &&
		typeof value.id === "string" &&
		"kind" in value &&
		(value.kind === "info" || value.kind === "error") &&
		"text" in value &&
		typeof value.text === "string" &&
		"at" in value &&
		typeof value.at === "number" &&
		"read" in value &&
		typeof value.read === "boolean" &&
		(!("detail" in value) || typeof value.detail === "string") &&
		(!("action" in value) || isNoticeAction(value.action))
	);
}

/// The notice list saved by an earlier start, when every notice in it is well formed. A list that is not is dropped.
export function isNoticeList(value: unknown): value is Notice[] {
	return Array.isArray(value) && value.every((item) => isNotice(item));
}

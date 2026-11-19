import { useCallback, useEffect, useRef, useState } from "react";

import { translateKey } from "@/i18n";
import { detailOf, messageOf } from "@/lib/ipc";
import { addNotice, continueToastRun, isNoticeList, MAX_NOTICES, type Notice, newNotice, readAll, type ToastRun } from "@/lib/notices";
import { type ToastAction, type ToastSeverity, toast } from "@/lib/notify";
import { readPref, writePref } from "@/lib/ui-prefs";

/// How long a success toast stays on the screen. The list keeps the notice until the user removes it.
const SUCCESS_TOAST_MS = 5000;

/// How long an error toast stays on the screen. The list keeps the error until the user removes it.
const ERROR_TOAST_MS = 10000;

const NOTICES_PREF = "notices";

/// The live toast and the run of identical results it stands for. Keeping the id lets a repeat update that toast in
/// place instead of stacking another card.
interface ToastFeed {
	run: ToastRun;
	id: string | number;
}

/// The notifications of the whole app. The list keeps every notice until the user removes it, newest first, and keeps it
/// across restarts; the bell and the notifications page show it. Each notice also pops up as a toast, which goes away on
/// its own. Ongoing work never reaches either surface: it lives in the Working panel.
export function useNotices() {
	const [notices, setNotices] = useState<Notice[]>(() => readPref<Notice[]>(NOTICES_PREF, [], isNoticeList).slice(0, MAX_NOTICES));
	const feed = useRef<ToastFeed | null>(null);
	useEffect(() => {
		writePref(NOTICES_PREF, notices.length > 0 ? notices : null);
	}, [notices]);

	const dismissNotice = useCallback((noticeId: string) => {
		setNotices((list) => list.filter((notice) => notice.id !== noticeId));
	}, []);

	/// Emits one toast per run of identical results, following `groupNotices` semantics. A repeat updates the same card
	/// with a "+N more" suffix; a different result starts a fresh card.
	const emitToast = useCallback((text: string, severity: ToastSeverity, action: ToastAction | undefined, timeout: number) => {
		const current = feed.current;
		const { run, repeated } = continueToastRun(current?.run ?? null, `${severity}\u0000${text}`);
		const title = repeated ? `${text} · ${translateKey("toasts.more", { count: run.count })}` : text;
		const id = toast.add({
			...(repeated && current ? { id: current.id } : {}),
			type: severity,
			title,
			action,
			timeout,
		});
		feed.current = { run, id };
	}, []);

	/// A completed one-line result: a toast, plus a persisted notice for later reference. The default severity keeps
	/// existing success call sites green; callers reporting neutral information pass `"info"` so the green check is
	/// reserved for real success. `action` is a button for the toast only — use `notifyDurable` to keep it.
	const notify = useCallback(
		(text: string, action?: ToastAction, severity: ToastSeverity = "success") => {
			setNotices((list) => addNotice(list, newNotice("info", text)));
			emitToast(text, severity, action, SUCCESS_TOAST_MS);
		},
		[emitToast],
	);

	/// A result whose recovery must outlive the toast (Undo): the toast carries the same button and the notification
	/// list keeps a durable row with it. A reload keeps the row but drops the button; the callback cannot be persisted.
	const notifyDurable = useCallback(
		(text: string, action: ToastAction) => {
			setNotices((list) => addNotice(list, newNotice("info", text, undefined, action)));
			emitToast(text, "success", action, SUCCESS_TOAST_MS);
		},
		[emitToast],
	);

	/// `detail` is the technical side of the failure, such as the backend's error code. It goes into the details.
	const notifyError = useCallback(
		(text: string, detail?: string, severity: ToastSeverity = "error") => {
			setNotices((list) => addNotice(list, newNotice("error", text, detail)));
			emitToast(text, severity, undefined, ERROR_TOAST_MS);
		},
		[emitToast],
	);

	const markRead = useCallback(() => setNotices(readAll), []);

	const clearNotices = useCallback(() => setNotices([]), []);

	/// Every backend action goes through here: an error goes to the notifications, not the console. The boolean says
	/// whether the action worked, so a caller can keep a dialog or a typed value on failure.
	const attempt = useCallback(
		async (action: () => Promise<void>): Promise<boolean> => {
			try {
				await action();
				return true;
			} catch (failure) {
				notifyError(messageOf(failure), detailOf(failure));
				return false;
			}
		},
		[notifyError],
	);

	return { notices, dismissNotice, notify, notifyDurable, notifyError, markRead, clearNotices, attempt };
}

import { createContext, type ReactNode, useContext, useMemo } from "react";
import { detailOf, messageOf } from "@/lib/ipc";
import type { Notice } from "@/lib/notices";
import type { ToastAction } from "@/lib/notify";
import { useNotices } from "@/lib/use-notices";

/// The callbacks every feature uses to report a result. They are stable, so a component that only reports does not
/// re-render when the list of notices changes.
interface NoticeActions {
	dismissNotice: (id: string) => void;
	notify: (text: string, action?: ToastAction) => void;
	/// A result whose recovery (Undo) must also stay in the notifications list after the toast is gone.
	notifyDurable: (text: string, action: ToastAction) => void;
	notifyError: (text: string, detail?: string) => void;
	/// Reports a failed call: its translated text, with the technical side as the detail.
	notifyFailure: (failure: unknown) => void;
	markRead: () => void;
	clearNotices: () => void;
	attempt: (action: () => Promise<void>) => Promise<boolean>;
}

const NoticeActionsContext = createContext<NoticeActions | null>(null);

export function useNoticeActions(): NoticeActions {
	const context = useContext(NoticeActionsContext);
	if (!context) {
		throw new Error("useNoticeActions must be used inside <NoticesProvider>.");
	}

	return context;
}

interface NoticeList {
	notices: Notice[];
}

const NoticeListContext = createContext<NoticeList | null>(null);

export function useNoticeList(): NoticeList {
	const context = useContext(NoticeListContext);
	if (!context) {
		throw new Error("useNoticeList must be used inside <NoticesProvider>.");
	}

	return context;
}

/// Owns the app-wide notification list. Kept in its own provider because every Tauri action can report a failure, while
/// only the rail and the notifications page read the list itself.
export function NoticesProvider({ children }: { children: ReactNode }) {
	const { notices, dismissNotice, notify, notifyDurable, notifyError, markRead, clearNotices, attempt } = useNotices();

	const actions = useMemo(
		(): NoticeActions => ({
			dismissNotice,
			notify,
			notifyDurable,
			notifyError,
			notifyFailure: (failure) => notifyError(messageOf(failure), detailOf(failure)),
			markRead,
			clearNotices,
			attempt,
		}),
		[attempt, clearNotices, dismissNotice, markRead, notify, notifyDurable, notifyError],
	);
	const list = useMemo((): NoticeList => ({ notices }), [notices]);

	return (
		<NoticeActionsContext.Provider value={actions}>
			<NoticeListContext.Provider value={list}>{children}</NoticeListContext.Provider>
		</NoticeActionsContext.Provider>
	);
}

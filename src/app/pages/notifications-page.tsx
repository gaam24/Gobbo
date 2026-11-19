import { Suspense } from "react";
import { useNoticeActions, useNoticeList } from "@/app/contexts/notices-context";
import { NotificationsView } from "@/app/lazy-views";
import { useTranslation } from "@/i18n";

/// The full notice list. It has no data of its own: the provider keeps the list and the page renders it.
export function NotificationsPage() {
	const { i18n } = useTranslation();
	const { notices } = useNoticeList();
	const { dismissNotice, clearNotices, markRead } = useNoticeActions();

	return (
		<Suspense fallback={null}>
			<NotificationsView notices={notices} locale={i18n.language} onDismiss={dismissNotice} onClear={clearNotices} onRead={markRead} />
		</Suspense>
	);
}

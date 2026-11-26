import { Bell, CircleCheck, CircleX, X } from "lucide-react";
import { useEffect } from "react";
import { Disclosure } from "@/components/app/content-blocks";
import { ListGroupHeader, ListRow, PageEmpty } from "@/components/app/page-layout";
import { PageFrame } from "@/components/page-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { formatDate, formatTime, isToday } from "@/lib/mods";
import { dismissGroup, groupNotices, type Notice, type NoticeGroup } from "@/lib/notices";

/// Every notice of the list, newest first. Notices that repeat one after another show once with a count; an error has
/// its technical details under it.
export function NotificationsView({
	notices,
	locale,
	onDismiss,
	onClear,
	onRead,
}: {
	/// Newest first: every notice that is still in the list.
	notices: Notice[];
	locale: string;
	onDismiss: (id: string) => void;
	onClear: () => void;
	onRead: () => void;
}) {
	const { t } = useTranslation();

	// The page shows everything, so what arrives while it is open is read at once. Depend on the list itself: a notice
	// can arrive between `markRead` and its commit while the boolean is still true.
	useEffect(() => {
		if (notices.some((notice) => !notice.read)) onRead();
	}, [notices, onRead]);

	const groups = groupNotices(notices);
	const days: Array<{ label: string; groups: NoticeGroup[] }> = [];

	for (const group of groups) {
		const date = formatDate(group.notice.at, locale);
		const label = isToday(group.notice.at) ? t("notifications.today") : date;
		const last = days.at(-1);

		if (last?.label === label) last.groups.push(group);
		else days.push({ label, groups: [group] });
	}

	return (
		<PageFrame
			title={t("notifications.title")}
			hint={t("notifications.hint")}
			meta={groups.length > 0 ? groups.length : undefined}
			action={
				notices.length > 0 && (
					<Button variant="outline" size="sm" onClick={onClear}>
						{t("notifications.clear")}
					</Button>
				)
			}
		>
			{groups.length === 0 ? (
				<PageEmpty icon={Bell} title={t("notifications.empty")} />
			) : (
				// One header per day, so each row only needs its time.
				days.map((day) => (
					<section key={day.label} aria-label={day.label}>
						<ListGroupHeader title={day.label} count={day.groups.length} />
						<ul>
							{day.groups.map((group) => (
								<NoticeRow key={group.notice.id} group={group} locale={locale} onDismiss={onDismiss} />
							))}
						</ul>
					</section>
				))
			)}
		</PageFrame>
	);
}

interface NoticeRowProps {
	group: NoticeGroup;
	locale: string;
	onDismiss: (id: string) => void;
}

function NoticeRow({ group, locale, onDismiss }: NoticeRowProps) {
	const { t } = useTranslation();
	const { notice, count } = group;

	return (
		<ListRow asChild className="items-start py-3">
			<li>
				<ItemMedia className="mt-0.5">
					{notice.kind === "error" ? (
						<CircleX className="text-destructive size-4" aria-hidden />
					) : (
						<CircleCheck className="text-muted-foreground size-4" aria-hidden />
					)}
				</ItemMedia>
				<ItemContent className="gap-1">
					<ItemTitle className="line-clamp-none w-full font-normal wrap-break-word whitespace-pre-line">{notice.text}</ItemTitle>
					<ItemDescription className="line-clamp-none flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
						{notice.kind === "error" && <span className="text-destructive font-medium">{t("app.actionFailed")}</span>}
						<span>{formatTime(notice.at, locale)}</span>
						{count > 1 && <Badge variant="outline">{t("notifications.repeated", { count })}</Badge>}
					</ItemDescription>
					{notice.detail && (
						<Disclosure
							className="text-muted-foreground text-xs"
							title={<span className="text-xs font-normal">{t("common.details")}</span>}
						>
							<p className="font-mono break-all">{notice.detail}</p>
						</Disclosure>
					)}
				</ItemContent>
				<ItemActions>
					{/* A durable action (Undo) outlives its toast. The callback is in memory only, so a notice restored
					    from storage has no button; hide it rather than render one that cannot do anything. */}
					{notice.action && typeof notice.action.onClick === "function" && (
						<Button variant="outline" size="sm" onClick={notice.action.onClick}>
							{notice.action.label}
						</Button>
					)}
					<Button variant="ghost" size="icon-sm" aria-label={t("notifications.dismiss")} onClick={() => dismissGroup(group, onDismiss)}>
						<X aria-hidden />
					</Button>
				</ItemActions>
			</li>
		</ListRow>
	);
}

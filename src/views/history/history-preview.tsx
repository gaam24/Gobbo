import { Fragment } from "react";
import type { HistoryPreview_Serialize, HistorySide_Serialize } from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { formatHistoryTime, hasPreviewChanges } from "@/lib/history";

const isMissing = (mod: { missing: boolean }) => mod.missing;
const versionOf = (mod: { version?: string | null; folder: string }) => mod.version ?? mod.folder;

/// What restoring the selected state changes, compared with the list as it is now. The dialog owns the restore button
/// and the load; this part only shows the answer.
export function HistoryPreviewPane({ preview, locale }: { preview: HistoryPreview_Serialize; locale: string }) {
	const { t } = useTranslation();
	const adds = preview.adds.filter((mod) => !mod.missing);
	const missing = preview.adds.filter((mod) => mod.missing);
	const counts = [
		{ key: "moved", value: preview.moved },
		{ key: "priority", value: preview.priority },
		{ key: "groups", value: preview.groups },
		{ key: "winners", value: preview.winners },
		{ key: "waiting", value: preview.waiting },
	] as const;
	const changed = hasPreviewChanges(preview);
	const modLine = (mod: HistorySide_Serialize) => `${versionOf(mod)} · ${mod.enabled ? t("mods.state.enabled") : t("mods.state.disabled")}`;

	return (
		<div className="grid content-start gap-4">
			<header className="grid gap-1">
				<h3 className="text-base font-semibold">{t("history.preview.title", { time: formatHistoryTime(preview.at, locale) })}</h3>
				<p className="text-muted-foreground text-sm">{t("history.preview.hint")}</p>
				<p className="text-muted-foreground text-xs tabular-nums">
					{t("history.preview.after", { enabled: preview.enabled, total: preview.total })}
				</p>
			</header>

			{missing.length > 0 && (
				<Alert>
					<AlertTitle>{t("history.preview.missing", { count: missing.length })}</AlertTitle>
					<AlertDescription>
						<p>{t("history.preview.missingHint")}</p>
						<BoxList>
							{missing.map((mod) => (
								<BoxListRow key={mod.id}>
									<ItemContent className="min-w-0">
										<ItemTitle className="w-full truncate font-normal">
											{mod.name}
											{" · "}
											{mod.version ?? mod.folder}
										</ItemTitle>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</AlertDescription>
				</Alert>
			)}

			{!changed && <p className="text-sm">{t("history.preview.same")}</p>}
			<ChangeGroup title={t("history.preview.adds", { count: adds.length })} items={adds} missing={isMissing} describe={modLine} />
			<ChangeGroup
				title={t("history.preview.removes", { count: preview.removes.length })}
				items={preview.removes}
				missing={isMissing}
				describe={modLine}
			/>
			<ChangeGroup
				title={t("history.preview.versions", { count: preview.versions.length })}
				items={preview.versions}
				missing={(change) => change.from.missing || change.to.missing}
				describe={(change) => `${modLine(change.from)} → ${modLine(change.to)}`}
			/>
			<ChangeGroup
				title={t("history.preview.switchesOn", { count: preview.switches_on.length })}
				items={preview.switches_on}
				missing={isMissing}
				describe={versionOf}
			/>
			<ChangeGroup
				title={t("history.preview.switchesOff", { count: preview.switches_off.length })}
				items={preview.switches_off}
				missing={isMissing}
				describe={versionOf}
			/>
			{counts.some(({ value }) => value > 0) && (
				<dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
					{counts.map(({ key, value }) =>
						value > 0 ? (
							<Fragment key={key}>
								<dt className="text-muted-foreground">{t(`history.preview.${key}`)}</dt>
								<dd className="tabular-nums">{value}</dd>
							</Fragment>
						) : null,
					)}
				</dl>
			)}
		</div>
	);
}

/// One kind of change in the preview: each row names a mod, says when it is not in the library, and describes the
/// change. Kept apart from the footer so an empty group takes no space and a long one folds.
function ChangeGroup<T extends { id: string; name: string }>({
	title,
	items,
	missing,
	describe,
}: {
	title: string;
	items: T[];
	missing: (item: T) => boolean;
	describe: (item: T) => string;
}) {
	const { t } = useTranslation();
	if (items.length === 0) return null;
	return (
		<Disclosure title={title} defaultOpen={items.length <= 8}>
			<BoxList scroll>
				{items.map((item) => (
					<BoxListRow key={item.id}>
						<ItemContent className="gap-0.5">
							<ItemTitle>
								{item.name}
								{missing(item) && <Badge variant="outline">{t("history.preview.notInLibrary")}</Badge>}
							</ItemTitle>
							<ItemDescription className="font-mono text-xs">{describe(item)}</ItemDescription>
						</ItemContent>
					</BoxListRow>
				))}
			</BoxList>
		</Disclosure>
	);
}

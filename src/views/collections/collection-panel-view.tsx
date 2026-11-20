import { ArrowUpCircle, Ban, ExternalLink, Play, RefreshCw, Trash2, Undo2 } from "lucide-react";
import { useMemo, useState } from "react";

import { useCollections } from "@/app/contexts/collections-context";
import { useSettingsActions } from "@/app/contexts/settings-context";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { Disclosure } from "@/components/app/content-blocks";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { useTranslation } from "@/i18n";
import { needsWork, orderedItems, revisionLabel } from "@/lib/collections";
import { CollectionMemberList } from "@/views/collections/collection-member-list";
import { CollectionTitle, collectionMeta } from "@/views/collections/collection-title";

/// The panel of a collection in a profile: its counts, every member with its state, install the missing ones, update to
/// a newer revision, open the page or remove the collection.
export function CollectionPanelView({ onClose }: { onClose: () => void }) {
	const { t, i18n } = useTranslation();
	const collections = useCollections();
	const { currentBrowserProviders } = useSettingsActions();
	const { deploy } = useWorkspaceActions();
	const [confirmRemove, setConfirmRemove] = useState(false);
	const session = collections.session;
	const items = useMemo(() => (session ? orderedItems(session.plan) : []), [session]);

	if (!session) {
		return (
			<ModalDialog title={t("collection.dialog.panel")} onClose={onClose}>
				<p className="text-sm">{t("collection.panel.gone")}</p>
				<DialogFooter>
					<Button variant="outline" onClick={onClose}>
						{t("common.close")}
					</Button>
				</DialogFooter>
			</ModalDialog>
		);
	}
	// A session view carries no route flag of its own, so the backend's provider route is read through the synchronous
	// accessor. It keeps a resume after a key save from using a browser route the backend has already changed.
	const canAuto = !currentBrowserProviders().has(session.origin.provider);
	const update = session.update;
	const pending = session.counts.pending;
	const busy = collections.busy;

	return (
		<ModalDialog
			width="3xl"
			className="h-[85vh]"
			title={<CollectionTitle name={session.origin.name} revision={revisionLabel(session.origin)} provider={session.origin.provider} />}
			description={collectionMeta(t, i18n.language, {
				...session.plan.info,
				author: session.origin.author,
			})}
			onClose={onClose}
		>
			<div className="flex h-full min-h-0 flex-col gap-4 text-sm">
				<div className="flex justify-end">
					<Button
						size="sm"
						variant="ghost"
						disabled={collections.checking || busy}
						onClick={() => void collections.checkUpdates(session.profile)}
					>
						<RefreshCw className={collections.checking ? "animate-spin" : undefined} /> {t("collection.panel.check")}
					</Button>
				</div>
				<div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
					<Count label={t("collection.status.installed")} value={session.counts.installed + session.counts.manual} tone="text-applied" />
					<Count label={t("collection.panel.pending")} value={pending} tone={pending > 0 ? "text-draft" : undefined} />
					<Count label={t("collection.status.skipped")} value={session.counts.skipped} />
					<Count
						label={t("collection.status.unsupported")}
						value={session.counts.unsupported}
						tone={session.counts.unsupported > 0 ? "text-destructive" : undefined}
					/>
				</div>
				{update && (
					<div className="border-update/40 flex flex-wrap items-center gap-3 rounded-lg border px-3 py-2">
						<ArrowUpCircle className="text-update size-4" />
						<span className="min-w-0 flex-1">{t("collection.panel.update")}</span>
						<Button size="sm" variant="outline" disabled={busy} onClick={() => void collections.openPreview(session.origin.url)}>
							{t("collection.panel.review")}
						</Button>
					</div>
				)}
				{session.plan.info.changelog && (
					<Disclosure title={t("collection.panel.changelog")}>
						<p className="text-muted-foreground max-h-40 overflow-auto whitespace-pre-wrap">{session.plan.info.changelog}</p>
					</Disclosure>
				)}
				<CollectionMemberList
					items={items}
					statuses={session.statuses}
					provider={session.origin.provider}
					className="min-h-48 flex-1 grid-rows-[auto_minmax(0,1fr)]"
					renderAction={(item) => {
						const status = session.statuses[item.key];

						if (status?.kind === "skipped") {
							return (
								<Button
									size="icon-sm"
									variant="ghost"
									disabled={busy}
									aria-label={t("collection.panel.restore")}
									title={t("collection.panel.restore")}
									onClick={() => void collections.skipItem(item.key, false)}
								>
									<Undo2 />
								</Button>
							);
						}

						if (needsWork(status)) {
							return (
								<Button
									size="icon-sm"
									variant="ghost"
									disabled={busy}
									aria-label={t("collection.panel.skip")}
									title={t("collection.panel.skip")}
									onClick={() => void collections.skipItem(item.key, true)}
								>
									<Ban />
								</Button>
							);
						}

						return <span className="size-7 shrink-0" />;
					}}
				/>
			</div>
			<ConfirmDialog
				open={confirmRemove}
				onOpenChange={setConfirmRemove}
				title={t("collection.panel.removeTitle")}
				description={t("collection.panel.removeHint")}
				confirmLabel={t("collection.panel.remove")}
				onConfirm={() => {
					void collections.removeCollection(session);
					onClose();
				}}
			/>
			<DialogFooter>
				<Button variant="ghost" className="text-destructive sm:mr-auto" onClick={() => setConfirmRemove(true)}>
					<Trash2 /> {t("collection.panel.remove")}
				</Button>
				<Button variant="ghost" onClick={() => void collections.openPage(session.profile, session.origin)}>
					<ExternalLink /> {t("collection.panel.openPage")}
				</Button>
				<Button variant="outline" onClick={onClose}>
					{t("common.close")}
				</Button>
				{pending > 0 ? (
					<Button disabled={busy} onClick={() => void collections.resume(session, canAuto ? "automatic" : "guided")}>
						<RefreshCw /> {t("collection.panel.installMissing", { count: pending })}
					</Button>
				) : (
					<Button
						onClick={() => {
							onClose();
							deploy();
						}}
					>
						<Play /> {t("collection.panel.apply")}
					</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}

function Count({ label, value, tone }: { label: string; value: number; tone?: string }) {
	return (
		<div className="rounded-lg border px-3 py-2">
			<div className="text-muted-foreground text-xs">{label}</div>
			<div className={`font-mono text-lg font-semibold tabular-nums ${tone ?? ""}`}>{value}</div>
		</div>
	);
}

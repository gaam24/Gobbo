import { CheckCircle2, CircleAlert, Download, FileArchive, Play, RefreshCw } from "lucide-react";
import { useMemo } from "react";

import { useCollections } from "@/app/contexts/collections-context";
import { useSettingsActions } from "@/app/contexts/settings-context";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { ARCHIVE_EXTENSIONS } from "@/lib/archives";
import { collectionItemLabel, orderedItems, revisionLabel } from "@/lib/collections";
import { errorText, translate } from "@/lib/ipc";
import { pickFiles } from "@/lib/tauri";
import { CollectionMemberList } from "@/views/collections/collection-member-list";
import { CollectionTitle } from "@/views/collections/collection-title";

const STATE_LABEL = {
	downloading: "collection.state.downloading",
	importing: "collection.state.importing",
	patching: "collection.state.patching",
	installed: "collection.state.installed",
	failed: "collection.state.failed",
	needs_choice: "collection.state.needsChoice",
	needs_user: "collection.state.needsUser",
	skipped: "collection.state.skipped",
} as const;

/// The install progress: one bar, the member that needs the user now, and the live state of every member. The FOMOD
/// wizard replaces this dialog in the parent when a member needs choices.
export function CollectionInstallView({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	const collections = useCollections();
	const { currentBrowserProviders } = useSettingsActions();
	const { deploy } = useWorkspaceActions();
	const session = collections.session;
	const progress = collections.progress;
	const items = useMemo(() => (session ? orderedItems(session.plan) : []), [session]);

	if (!session) {
		return (
			<ModalDialog title={t("collection.dialog.install")} onClose={onClose}>
				<p className="text-sm">{collections.failure ?? t("collection.empty")}</p>
				<DialogFooter>
					<Button variant="outline" onClick={onClose}>
						{t("common.close")}
					</Button>
				</DialogFooter>
			</ModalDialog>
		);
	}
	// A session view carries no route flag of its own, so the backend's provider route is read through the synchronous
	// accessor. It keeps a continue after a key save from using a browser route the backend has already changed.
	const canAuto = !currentBrowserProviders().has(session.origin.provider);
	const wanted = session.counts.total - session.counts.skipped - session.counts.unsupported;
	const done = session.counts.installed + session.counts.manual;
	const percent = wanted > 0 ? Math.min(100, Math.round((done * 100) / wanted)) : 100;
	const finished = session.counts.pending === 0;
	const waitingName = collections.waiting ? collectionItemLabel(items, collections.waiting.item) : null;
	const manualName = collections.manual ? collectionItemLabel(items, collections.manual.item) : null;
	const idle = !collections.busy && collections.waiting === null && collections.manual === null;
	const failed = collections.installView?.failed ?? [];
	const warnings = collections.installView?.warnings ?? [];
	const liveState = (item: { key: string }) => {
		const step = progress.get(item.key);

		if (step) return step.percent > 0 && step.percent < 100 ? `${t(STATE_LABEL[step.state])} ${step.percent}%` : t(STATE_LABEL[step.state]);

		if (collections.waiting?.item === item.key) return t("collection.state.waiting");
		return null;
	};
	const stateText = () => {
		if (finished) return t("collection.install.done");

		if (collections.busy) return t("collection.install.working");

		if (collections.installView?.cancelled) return t("collection.state.paused");
		return t("collection.install.idle");
	};
	const chooseManual = async () => {
		const [path] = await pickFiles(t("collection.fileFilter"), ARCHIVE_EXTENSIONS, false);

		if (path) await collections.importManual(path);
	};

	return (
		<ModalDialog
			width="3xl"
			className="h-[85vh]"
			title={
				<CollectionTitle name={session.plan.info.name} revision={revisionLabel(session.origin)} provider={session.origin.provider}>
					{collections.mode && <Badge variant="outline">{t(`collection.mode.short.${collections.mode}`)}</Badge>}
				</CollectionTitle>
			}
			description={t("collection.install.progress", {
				done,
				total: wanted,
				pending: session.counts.pending,
			})}
			onClose={onClose}
		>
			<div className="flex h-full min-h-0 flex-col gap-4 text-sm">
				<div className="grid gap-1.5">
					<Progress value={percent} aria-label={t("collection.install.percent", { percent })} />
					<span className="text-muted-foreground flex items-center gap-1.5 text-xs">
						{collections.busy && <Spinner className="size-3" />}
						{stateText()}
					</span>
				</div>
				{collections.waiting && (
					<div className="border-primary/40 bg-primary/5 grid gap-2 rounded-lg border p-3">
						<div className="flex items-center gap-2 font-medium">
							<Download className="size-4" /> {t("collection.install.waiting", { name: waitingName ?? "" })}
						</div>
						<p className="text-muted-foreground text-xs">{t("collection.install.freeHint")}</p>
						<div className="flex flex-wrap gap-2">
							<Button size="sm" onClick={() => void collections.guide()}>
								<Download /> {t("collection.install.openAgain")}
							</Button>
							<Button size="sm" variant="outline" onClick={() => void collections.skip(false)}>
								{t("collection.install.skipNow")}
							</Button>
							<Button size="sm" variant="ghost" onClick={() => void collections.skip(true)}>
								{t("collection.install.dontInstall")}
							</Button>
						</div>
					</div>
				)}
				{collections.manual && (
					<div className="border-draft/40 grid gap-2 rounded-lg border p-3">
						<div className="flex items-center gap-2 font-medium">
							<FileArchive className="size-4" /> {t("collection.install.manual", { name: manualName ?? "" })}
						</div>
						{collections.manual.url && <p className="text-muted-foreground font-mono text-xs break-all">{collections.manual.url}</p>}
						{collections.manual.instructions && (
							<p className="text-muted-foreground max-h-32 overflow-auto text-xs whitespace-pre-wrap">{collections.manual.instructions}</p>
						)}
						<div className="flex flex-wrap gap-2">
							<Button size="sm" onClick={() => void chooseManual()}>
								<FileArchive /> {t("collection.install.chooseFile")}
							</Button>
							<Button size="sm" variant="outline" onClick={() => void collections.markDone()}>
								<CheckCircle2 /> {t("collection.install.markDone")}
							</Button>
							<Button size="sm" variant="ghost" onClick={() => void collections.skip(true)}>
								{t("collection.install.dontInstall")}
							</Button>
						</div>
					</div>
				)}
				{failed.length > 0 && (
					<Disclosure
						title={
							<span className="text-destructive inline-flex items-center gap-2 font-medium">
								<CircleAlert className="size-4" /> {t("collection.install.failed", { count: failed.length })}
							</span>
						}
					>
						<BoxList scroll className="max-h-32 text-xs">
							{failed.map((failure) => (
								<BoxListRow key={failure.key}>
									<ItemContent>
										<ItemTitle className="text-xs">{failure.name}</ItemTitle>
										<ItemDescription className="line-clamp-none text-xs">{errorText(failure.error)}</ItemDescription>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</Disclosure>
				)}
				{warnings.length > 0 && (
					<Disclosure
						title={
							<span className="text-muted-foreground inline-flex items-center gap-2 font-medium">
								<CircleAlert className="size-4" /> {t("collection.install.warnings")}
							</span>
						}
					>
						<BoxList scroll className="max-h-32 text-xs">
							{warnings.map((warning) => (
								<BoxListRow key={`${warning.code}-${warning.params.name ?? ""}`}>
									<ItemContent>
										<ItemDescription className="line-clamp-none text-xs">{translate(warning)}</ItemDescription>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</Disclosure>
				)}
				<CollectionMemberList
					items={items}
					statuses={session.statuses}
					provider={session.origin.provider}
					renderState={liveState}
					className="min-h-48 flex-1 grid-rows-[auto_minmax(0,1fr)]"
				/>
				{collections.failure && (
					<p role="alert" className="text-destructive text-sm">
						{collections.failure}
					</p>
				)}
			</div>
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("collection.install.close")}
				</Button>
				<Button
					variant={finished ? "default" : "outline"}
					disabled={!finished}
					onClick={() => {
						onClose();
						deploy();
					}}
				>
					<Play /> {t("collection.install.apply")}
				</Button>
				{idle && !finished && (
					<Button onClick={() => void collections.resume(session, canAuto ? "automatic" : "guided")}>
						<RefreshCw /> {t("collection.install.continue")}
					</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}

import { ArchiveRestore, CircleAlert, History, Trash2 } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands, type RestorePreview, type SnapshotView } from "@/bindings";
import { BoxList, BoxListRow, StatusLine } from "@/components/app/content-blocks";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { ItemActions, ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { formatDateTime, formatSize } from "@/lib/mods";
import { registerTask, removeTask } from "@/lib/tasks-store";
import { useLoad } from "@/lib/use-load";

/// Save snapshots of the active profile: create one now, restore an earlier one after a preview, or delete one. The
/// list gets a fresh key after every change, so it always shows the result of the action that just ran.
export function SnapshotDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t, i18n } = useTranslation();
	const { status, gameRunning } = useWorkspace();
	const active = status?.profiles.find((profile) => profile.active);
	const [version, setVersion] = useState(0);
	const [busy, setBusy] = useState<string | null>(null);
	const [failure, setFailure] = useState<string | null>(null);
	const [notice, setNotice] = useState<string | null>(null);
	const [pending, setPending] = useState<{
		snapshot: SnapshotView;
		preview: RestorePreview;
	} | null>(null);
	const [deleting, setDeleting] = useState<SnapshotView | null>(null);
	const snapshots = useLoad(`snapshots:${gameId}:${version}`, () => unwrap(commands.snapshots(gameId)));

	const run = async (key: string, label: string, action: () => Promise<string | null>) => {
		const id = `snapshot:${gameId}|${key}`;
		registerTask({
			id,
			game: gameId,
			kind: "install",
			title: t("snapshots.title"),
			detail: label,
			state: "running",
			source: "local",
		});
		setBusy(key);
		setFailure(null);
		setNotice(null);

		try {
			setNotice(await action());
			setVersion((count) => count + 1);
		} catch (error) {
			setFailure(messageOf(error));
		}
		// Both the success and the failure path end the busy state here; React Compiler skips a `finally`.
		removeTask(id);
		setBusy(null);
	};

	const create = () => {
		if (!active) return;
		void run("create", t("snapshots.creating"), async () => {
			await unwrap(commands.createSnapshot(gameId, active.id));
			return t("snapshots.created");
		});
	};

	const askRestore = (snapshot: SnapshotView) => {
		void run(snapshot.id, t("snapshots.restoring"), async () => {
			const preview = await unwrap(commands.previewSnapshotRestore(gameId, snapshot.id));
			setPending({ snapshot, preview });
			return null;
		});
	};

	const restore = () => {
		if (!pending) return;
		void run(pending.snapshot.id, t("snapshots.restoring"), async () => {
			const outcome = await unwrap(commands.restoreSnapshot(gameId, pending.snapshot.id));
			setPending(null);
			return outcome.profile_created
				? t("snapshots.restoredProfile", {
						profile: pending.snapshot.profile_name,
						files: outcome.files,
					})
				: t("snapshots.restored", { files: outcome.files });
		});
	};

	const remove = (snapshot: SnapshotView) => {
		void run(snapshot.id, t("common.delete"), async () => {
			await unwrap(commands.deleteSnapshot(gameId, snapshot.id));
			setDeleting(null);
			return t("snapshots.deleted");
		});
	};

	const canRestore = pending !== null && !pending.preview.running && pending.preview.enough_space && busy === null;

	let listBody: ReactNode;

	if (!snapshots) {
		listBody = <StatusLine tone="loading">{t("snapshots.loading")}</StatusLine>;
	} else if (snapshots.length === 0) {
		listBody = <StatusLine>{t("snapshots.empty")}</StatusLine>;
	} else {
		listBody = (
			<BoxList scroll>
				{snapshots.map((snapshot) => (
					<BoxListRow key={snapshot.id} className="flex-wrap">
						<ItemContent className="min-w-0">
							<ItemTitle className="w-full font-normal">
								{formatDateTime(snapshot.created_at * 1000, i18n.language)}
								{" · "}
								{t(`snapshots.kind.${snapshot.kind}`)}
							</ItemTitle>
							<ItemDescription className="line-clamp-none">
								{t("snapshots.details", {
									profile: snapshot.profile_name,
									mods: snapshot.mods,
									files: snapshot.files,
									size: formatSize(Math.ceil(snapshot.bytes / 1024), i18n.language),
								})}
							</ItemDescription>
							{snapshot.overwrite_files > 0 && (
								<ItemDescription className="line-clamp-none">
									{t("snapshots.overwrite", {
										files: snapshot.overwrite_files,
										size: formatSize(Math.ceil(snapshot.overwrite_bytes / 1024), i18n.language),
									})}
								</ItemDescription>
							)}
						</ItemContent>
						<ItemActions>
							<Button
								variant="outline"
								size="sm"
								disabled={busy !== null || snapshot.kind === "before_restore"}
								onClick={() => askRestore(snapshot)}
							>
								<History aria-hidden />
								{t("snapshots.restore")}
							</Button>
							<Button variant="ghost" size="sm" aria-label={t("common.delete")} onClick={() => setDeleting(snapshot)}>
								<Trash2 aria-hidden />
							</Button>
						</ItemActions>
					</BoxListRow>
				))}
			</BoxList>
		);
	}

	return (
		<>
			<ModalDialog title={t("snapshots.title")} description={t("snapshots.hint")} width="xl" onClose={onClose}>
				{/* The button names the profile it saves, and the reason it waits sits right under it. */}
				<div className="grid justify-items-start gap-1.5">
					<Button disabled={!active || gameRunning || busy !== null} onClick={create}>
						<ArchiveRestore aria-hidden />
						{busy === "create" ? t("snapshots.creating") : t("snapshots.createFor", { name: active?.name ?? "" })}
					</Button>
					{gameRunning && <p className="text-muted-foreground text-xs">{t("snapshots.gameRunning")}</p>}
				</div>

				{pending && (
					<div className="bg-muted/40 grid gap-2 rounded-lg border p-3">
						<p className="text-sm">
							{t("snapshots.preview", {
								added: pending.preview.added,
								replaced: pending.preview.replaced,
								removed: pending.preview.removed,
							})}
						</p>
						<p className="text-muted-foreground text-xs">
							{t("snapshots.previewSize", {
								size: formatSize(Math.ceil(pending.preview.bytes / 1024), i18n.language),
							})}
						</p>
						<p className="text-muted-foreground text-xs">{t("snapshots.previewProfile", { mods: pending.preview.mods })}</p>
						{pending.preview.recreates_profile && (
							<p className="text-sm">
								{t("snapshots.recreatesProfile", {
									profile: pending.snapshot.profile_name,
								})}
							</p>
						)}
						{!pending.preview.enough_space && (
							<FieldError>
								{t("snapshots.noSpace", {
									needed: formatSize(Math.ceil(pending.preview.bytes / 1024), i18n.language),
									free: formatSize(pending.preview.free_kb, i18n.language),
								})}
							</FieldError>
						)}
						{pending.preview.running && <StatusLine tone="error">{t("snapshots.gameRunning")}</StatusLine>}
						<div className="flex flex-wrap gap-2">
							<Button size="sm" disabled={!canRestore} onClick={restore}>
								<History aria-hidden />
								{busy === pending.snapshot.id ? t("snapshots.restoring") : t("snapshots.restore")}
							</Button>
							<Button variant="ghost" size="sm" onClick={() => setPending(null)}>
								{t("common.cancel")}
							</Button>
						</div>
					</div>
				)}

				{listBody}

				{failure && (
					<FieldError className="flex items-start gap-2">
						<CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
						{failure}
					</FieldError>
				)}
				{notice && (
					<p role="status" className="text-muted-foreground text-sm">
						{notice}
					</p>
				)}
				<DialogCloseFooter onClose={onClose} />
			</ModalDialog>
			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => !open && setDeleting(null)}
				title={t("snapshots.deleteTitle")}
				description={t("snapshots.deleteBody")}
				onConfirm={() => {
					if (deleting) remove(deleting);
				}}
			/>
		</>
	);
}

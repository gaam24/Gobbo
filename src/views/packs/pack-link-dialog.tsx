import { Channel } from "@tauri-apps/api/core";
import { Link as LinkIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { commands, type PackLinkProgress } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { registerTask, removeTask, updateTask } from "@/lib/tasks-store";

/// Downloads a pack link into the pack preview. The hash and size are checked before the preview opens; a cancel
/// leaves no file behind. The pack format itself is unchanged, so the existing preview and import do the rest.
export function PackLinkDialog({ gameId, link: initial, onClose }: { gameId: string; link: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { openForGame } = useWorkspaceActions();
	const [link, setLink] = useState(initial);
	const [busy, setBusy] = useState(false);
	const [percent, setPercent] = useState<number | null>(null);
	const [failure, setFailure] = useState<string | null>(null);
	/// The command keeps running after the dialog closes. Progress and the Working task update through module functions,
	/// but opening the preview is a UI action that must not fire for a dialog that no longer exists.
	const mounted = useRef(true);
	useEffect(() => {
		mounted.current = true;
		return () => {
			mounted.current = false;
		};
	}, []);

	const download = async () => {
		const wanted = link.trim();
		const id = `pack-link:${gameId}|${wanted}`;
		setBusy(true);
		setFailure(null);
		setPercent(0);
		registerTask({
			id,
			game: gameId,
			kind: "download",
			title: t("tasks.packDownload"),
			detail: wanted,
			percent: 0,
			state: "running",
			cancel: {
				label: t("common.cancel"),
				run: () => void commands.cancelPackLink(gameId, wanted),
			},
			open: {
				label: t("tasks.openWindow"),
				run: () => openForGame(gameId, { kind: "pack-link", link: wanted }),
			},
			source: "local",
		});
		const progress = new Channel<PackLinkProgress>();
		// `onmessage` is the Channel class callback from the Tauri API, not a DOM event handler.
		// oxlint-disable-next-line unicorn/prefer-add-event-listener
		progress.onmessage = (step) => {
			setPercent(step.percent);
			updateTask(id, { percent: step.percent });
		};

		try {
			const result = await unwrap(commands.downloadPackLink(gameId, wanted, progress));

			if (mounted.current) {
				removeTask(id);
				openForGame(gameId, { kind: "pack-import", path: result.path });
			} else {
				// Backgrounded: leave a task that reopens the ready preview instead of silently finishing.
				updateTask(id, {
					state: "waiting",
					percent: 100,
					detail: result.path,
					open: {
						label: t("tasks.openWindow"),
						run: () => {
							removeTask(id);
							openForGame(gameId, { kind: "pack-import", path: result.path });
						},
					},
				});
			}
		} catch (error) {
			updateTask(id, { state: "failed", detail: messageOf(error) });

			if (mounted.current) setFailure(messageOf(error));
		}
		// Both the success and the failure path clear the progress here; React Compiler skips a `finally`.
		setBusy(false);
		setPercent(null);
	};

	const cancel = () => {
		void commands.cancelPackLink(gameId, link.trim());
	};

	return (
		<ModalDialog title={t("pack.link.title")} description={t("pack.link.hint")} width="lg" onClose={onClose}>
			<Field>
				<FieldLabel htmlFor="pack-link" className="text-muted-foreground text-xs">
					{t("pack.link.label")}
				</FieldLabel>
				<Input
					id="pack-link"
					value={link}
					placeholder={t("pack.link.placeholder")}
					spellCheck={false}
					autoFocus
					onChange={(event) => setLink(event.currentTarget.value)}
				/>
			</Field>
			{percent !== null && <Progress value={percent} aria-label={t("pack.link.getting", { percent })} />}
			{failure && <FieldError>{failure}</FieldError>}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.close")}
				</Button>
				{busy && (
					<Button variant="outline" onClick={cancel}>
						{t("pack.link.cancel")}
					</Button>
				)}
				<Button disabled={busy || link.trim() === ""} onClick={() => void download()}>
					<LinkIcon aria-hidden />
					{busy ? t("pack.link.getting", { percent: percent ?? 0 }) : t("pack.link.get")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

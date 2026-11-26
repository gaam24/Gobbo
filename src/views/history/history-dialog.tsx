import { type ReactNode, useState } from "react";
import { commands, type HistoryEntry_Serialize } from "@/bindings";
import { StatusLine } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { hasPreviewChanges } from "@/lib/history";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";
import { HistoryPreviewPane } from "@/views/history/history-preview";
import { HistoryTimeline } from "@/views/history/history-timeline";

/// The history of the active profile's list. The left side is the change log; the right side shows what restoring
/// the selected state changes now, before the button is pressed.
export function HistoryDialog({
	gameId,
	profileName,
	total,
	enabled,
	locale,
	onRestore,
	onClose,
}: {
	gameId: string;
	profileName: string;
	/// The current list, for the "Now" row.
	total: number;
	enabled: number;
	locale: string;
	onRestore: (id: number) => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [selected, setSelected] = useState<number | null>(null);
	const [failure, setFailure] = useState<string | null>(null);
	const [previewFailure, setPreviewFailure] = useState<string | null>(null);
	const entries = useLoad(`${gameId}\n${profileName}`, () => unwrap(commands.listHistory(gameId)), {
		onError: (error) => setFailure(messageOf(error)),
		onSuccess: () => setFailure(null),
	});

	const selectedEntry: HistoryEntry_Serialize | null = entries?.find((entry) => entry.id === selected) ?? entries?.[0] ?? null;
	const selectedId = selectedEntry?.id ?? null;
	// The preview loads for the selected entry only; a new selection starts a new load, so an old answer is never shown.
	const preview = useLoad(
		selectedId === null ? null : `${gameId}\n${selectedId}`,
		() => unwrap(commands.previewRestore(gameId, selectedId ?? 0)),
		{
			onError: (error) => setPreviewFailure(messageOf(error)),
			onSuccess: () => setPreviewFailure(null),
		},
	);
	const restore = (id: number) => {
		onClose();
		void onRestore(id);
	};

	let previewBody: ReactNode;

	if (selectedEntry === null) {
		previewBody = (
			<Empty className="border border-dashed p-6">
				<EmptyHeader>
					<EmptyTitle>{t("history.empty")}</EmptyTitle>
				</EmptyHeader>
				<EmptyDescription>{t("history.emptyHint")}</EmptyDescription>
			</Empty>
		);
	} else if (previewFailure !== null && preview === undefined) {
		previewBody = <FieldError>{previewFailure}</FieldError>;
	} else if (preview === undefined) {
		previewBody = <StatusLine tone="loading">{t("history.preview.loading")}</StatusLine>;
	} else {
		previewBody = <HistoryPreviewPane preview={preview} locale={locale} />;
	}

	let body: ReactNode;

	if (failure !== null && entries === undefined) {
		body = <FieldError>{failure}</FieldError>;
	} else if (entries === undefined) {
		body = <StatusLine tone="loading">{t("history.loading")}</StatusLine>;
	} else {
		body = (
			<div className="grid gap-4 sm:grid-cols-[minmax(0,17rem)_minmax(0,1fr)]">
				<div className="sm:max-h-[60vh] sm:min-h-0 sm:overflow-y-auto sm:pr-1">
					<HistoryTimeline
						entries={entries}
						selectedId={selectedId}
						onSelect={setSelected}
						total={total}
						enabled={enabled}
						locale={locale}
					/>
				</div>
				<div className="sm:max-h-[60vh] sm:min-h-0 sm:overflow-y-auto sm:pr-1">{previewBody}</div>
			</div>
		);
	}

	return (
		<ModalDialog title={t("history.title")} description={t("history.hint")} width="3xl" onClose={onClose}>
			<p className="text-muted-foreground text-xs">{t("history.profile", { name: profileName })}</p>
			{body}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				{preview !== undefined && (
					<Button disabled={!hasPreviewChanges(preview)} onClick={() => restore(preview.id)}>
						{t("history.restoreEntry")}
					</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}

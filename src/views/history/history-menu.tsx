import { History, Redo2, Undo2 } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import type { HistoryState } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { useTranslation } from "@/i18n";

const HistoryDialog = lazy(async () => {
	const { HistoryDialog: component } = await import("@/views/history/history-dialog");
	return { default: component };
});

/// Undo, redo, and the earlier states of the list. A restore can itself be undone, so nothing here loses a list.
export function HistoryMenu({
	gameId,
	profileName,
	total,
	enabled,
	history,
	locale,
	onUndo,
	onRedo,
	onRestore,
}: {
	gameId: string;
	profileName: string;
	/// The current list, for the "Now" row of the history dialog.
	total: number;
	enabled: number;
	history: HistoryState;
	locale: string;
	onUndo: () => void | Promise<void>;
	onRedo: () => void | Promise<void>;
	onRestore: (id: number) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);

	return (
		<>
			<ButtonGroup>
				<Button
					variant="ghost"
					size="icon"
					aria-label={t("history.undo")}
					title={t("history.undo")}
					disabled={!history.can_undo}
					onClick={() => void onUndo()}
				>
					<Undo2 />
				</Button>
				<Button
					variant="ghost"
					size="icon"
					aria-label={t("history.redo")}
					title={t("history.redo")}
					disabled={!history.can_redo}
					onClick={() => void onRedo()}
				>
					<Redo2 />
				</Button>
				<Button
					variant="ghost"
					size="icon"
					aria-label={t("history.title")}
					title={t("history.title")}
					aria-haspopup="dialog"
					onClick={() => setOpen(true)}
				>
					<History />
				</Button>
			</ButtonGroup>
			<Suspense fallback={null}>
				{open && (
					<HistoryDialog
						gameId={gameId}
						profileName={profileName}
						total={total}
						enabled={enabled}
						locale={locale}
						onRestore={onRestore}
						onClose={() => setOpen(false)}
					/>
				)}
			</Suspense>
		</>
	);
}

import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import type { UpdateTarget } from "@/sources/update-targets";

/// The preview of **Update all**: every mod with a newer version, its current and newest version, before anything is
/// downloaded. A mod whose newest version is installed already is listed too, because the run only selects it.
export function UpdateAllDialog({
	open,
	onOpenChange,
	targets,
	manualCount,
	onConfirm,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	targets: UpdateTarget[];
	/// How many of the targets open their files page instead of downloading directly.
	manualCount: number;
	onConfirm: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<ModalDialog
			open={open}
			width="sm"
			title={t("updates.previewTitle", { count: targets.length })}
			description={manualCount > 0 ? t("updates.previewHintManual", { count: manualCount }) : t("updates.previewHint")}
			onClose={() => onOpenChange(false)}
		>
			<BoxList scroll>
				{targets.map((target) => (
					<BoxListRow key={target.id}>
						<ItemContent className="min-w-0 flex-row items-center justify-between gap-3">
							<ItemTitle className="min-w-0 truncate font-normal">{target.name}</ItemTitle>
							<span className="text-muted-foreground shrink-0 font-mono text-xs">
								{target.current ?? "?"} {"→"} {target.latest ?? "?"}
							</span>
						</ItemContent>
					</BoxListRow>
				))}
			</BoxList>
			<DialogFooter>
				<Button variant="outline" onClick={() => onOpenChange(false)}>
					{t("common.cancel")}
				</Button>
				<Button
					onClick={() => {
						onOpenChange(false);
						void onConfirm();
					}}
				>
					{t("updates.previewAction", { count: targets.length })}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

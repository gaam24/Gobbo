import type { ManualUpdatesView } from "@/app/hooks/use-manual-updates";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { type TranslationKey, useTranslation } from "@/i18n";

const STEP_LABEL: Record<ManualUpdatesView["steps"][string], TranslationKey> = {
	waiting: "updates.manual.state.waiting",
	downloading: "updates.manual.state.downloading",
	installing: "updates.manual.state.installing",
	selecting: "updates.manual.state.selecting",
	selected: "updates.manual.state.selected",
	failed: "updates.manual.state.failed",
	skipped: "updates.manual.state.skipped",
};

/// The view of the browser step of **Update all**. The sequence itself lives in `useManualUpdates` so it survives the
/// dialog closing and a game switch; this component only draws the controller's state.
export function ManualUpdateDialog({
	view,
	onNext,
	onFinish,
	onOpenPage,
	onClose,
}: {
	view: ManualUpdatesView;
	onNext: () => void;
	onFinish: () => void;
	onOpenPage: () => void;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const { targets, steps, errors, index, current, status } = view;

	return (
		<ModalDialog title={t("updates.manual.title")} description={t("updates.manual.hint")} width="sm" onClose={onClose}>
			<p className="text-muted-foreground text-sm">
				{t("updates.manual.step", {
					done: Math.min(index + 1, targets.length),
					total: targets.length,
				})}
			</p>
			<BoxList ordered scroll>
				{targets.map((target, at) => {
					const step = steps[target.id] ?? "waiting";
					return (
						<BoxListRow
							key={target.id}
							aria-current={at === index ? "step" : undefined}
							className="data-[current=true]:bg-muted"
							data-current={at === index}
						>
							<ItemContent className="min-w-0 gap-0.5">
								<span className="flex min-w-0 items-center justify-between gap-2">
									<ItemTitle className="min-w-0 truncate font-medium">{target.name}</ItemTitle>
									<span className="text-muted-foreground shrink-0 font-mono text-xs">
										{target.current ?? "?"} {"→"} {target.latest ?? "?"}
									</span>
								</span>
								<ItemDescription className={step === "failed" ? "text-destructive text-xs" : "text-muted-foreground text-xs"}>
									{step === "failed" ? (errors[target.id] ?? t("updates.manual.failedShort")) : t(STEP_LABEL[step])}
								</ItemDescription>
							</ItemContent>
						</BoxListRow>
					);
				})}
			</BoxList>
			<p className="text-sm" aria-live="polite">
				{status}
			</p>
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("updates.manual.close")}
				</Button>
				{current && (
					<Button variant="outline" onClick={onOpenPage}>
						{t("updates.manual.open")}
					</Button>
				)}
				{index < targets.length ? (
					<Button variant="outline" onClick={onNext}>
						{t("updates.manual.next")}
					</Button>
				) : (
					<Button onClick={onFinish}>{t("updates.manual.finish")}</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}

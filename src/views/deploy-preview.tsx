import { ArrowUpDown, FileStack, Files, Minus, Plus, RefreshCw } from "lucide-react";
import { type ReactNode, useState } from "react";
import { commands, type DeployPreview } from "@/bindings";
import { BoxList, BoxListRow, Disclosure, OutcomeRow, StatusLine } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, translate, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// The change lists a preview draws, in their displayed order. The pack dialog uses the same three.
export const PREVIEW_KEYS = ["added", "removed", "updated"] as const;

/// What a deployment will do, before it does it: the mods it adds, removes and changes, the files it places, and what
/// would stop it. Nothing is written until the user confirms.
export function DeployPreviewDialog({
	gameId,
	launch,
	request,
	onConfirm,
	onClose,
}: {
	gameId: string;
	/// The deployment starts the game afterwards.
	launch: boolean;
	/// Each request opens a fresh preview.
	request: number;
	/// Applies the mods.
	onConfirm: () => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	const preview = useLoad(`${gameId}\n${request}`, () => unwrap(commands.previewDeployment(gameId)), {
		onError: (error) => setFailure(messageOf(error)),
	});
	const blocked = (preview?.problems.length ?? 0) > 0;
	let body: ReactNode;

	if (preview !== undefined) body = <PreviewBody preview={preview} />;
	else if (failure) body = <FieldError>{failure}</FieldError>;
	else body = <StatusLine tone="loading">{t("preview.loading")}</StatusLine>;

	return (
		<ModalDialog title={t(launch ? "preview.titleLaunch" : "preview.title")} description={t("preview.hint")} width="lg" onClose={onClose}>
			{body}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button
					disabled={preview === undefined || blocked || failure !== null}
					onClick={() => {
						onClose();
						void onConfirm();
					}}
				>
					{t(launch ? "preview.confirmLaunch" : "preview.confirm")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

function PreviewBody({ preview }: { preview: DeployPreview }) {
	const { t } = useTranslation();
	const lists = PREVIEW_KEYS.map((key) => ({ key, names: preview[key] }));

	return (
		<>
			{/* What the apply means for the game, one outcome per row; the names follow in the folds below. */}
			<BoxList>
				{preview.added.length > 0 && (
					<OutcomeRow icon={Plus} tone="primary" title={t("preview.outcome.added", { count: preview.added.length })} />
				)}
				{preview.removed.length > 0 && <OutcomeRow icon={Minus} title={t("preview.outcome.removed", { count: preview.removed.length })} />}
				{preview.updated.length > 0 && (
					<OutcomeRow icon={RefreshCw} title={t("preview.outcome.updated", { count: preview.updated.length })} />
				)}
				<OutcomeRow
					icon={FileStack}
					title={t("preview.outcome.files", { count: preview.files })}
					detail={t("preview.outcome.filesDetail")}
				/>
				{preview.conflicts > 0 && (
					<OutcomeRow
						icon={Files}
						tone="warning"
						title={t("preview.outcome.conflicts", { count: preview.conflicts })}
						detail={t("preview.outcome.conflictsDetail")}
					/>
				)}
				{preview.moved > 0 && <OutcomeRow icon={ArrowUpDown} title={t("preview.outcome.moved", { count: preview.moved })} />}
				{preview.winners_changed > 0 && (
					<OutcomeRow icon={Files} title={t("preview.outcome.winners", { count: preview.winners_changed })} />
				)}
			</BoxList>
			{preview.replaces && <p className="text-muted-foreground">{t("preview.replaces")}</p>}
			{lists.map(({ key, names }) =>
				names.length > 0 ? (
					<Disclosure key={key} title={t(`preview.${key}`)} count={names.length} defaultOpen={names.length <= 5}>
						<BoxList scroll>
							{names.map((name) => (
								<BoxListRow key={name}>
									<ItemContent className="min-w-0">
										<ItemTitle className="w-full truncate font-normal">{name}</ItemTitle>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</Disclosure>
				) : null,
			)}
			{preview.problems.length > 0 && (
				<Alert variant="destructive">
					<AlertTitle>{t("preview.problems")}</AlertTitle>
					<AlertDescription>
						<BoxList>
							{preview.problems.map((problem) => (
								<BoxListRow key={`${problem.code}${JSON.stringify(problem.params)}`}>
									<ItemContent className="min-w-0">
										<ItemTitle className="w-full font-normal whitespace-normal">{translate(problem)}</ItemTitle>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</AlertDescription>
				</Alert>
			)}
			{preview.warnings.length > 0 && (
				<Alert>
					<AlertTitle>{t("preview.warningsTitle")}</AlertTitle>
					<AlertDescription>
						<BoxList>
							{preview.warnings.map((warning) => (
								<BoxListRow key={`${warning.code}${JSON.stringify(warning.params)}`}>
									<ItemContent className="min-w-0">
										<ItemTitle className="w-full font-normal whitespace-normal">{translate(warning)}</ItemTitle>
									</ItemContent>
								</BoxListRow>
							))}
						</BoxList>
					</AlertDescription>
				</Alert>
			)}
		</>
	);
}

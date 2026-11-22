import { type ReactNode, useState } from "react";
import { commands, type DeployPreview } from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
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
	else body = <p className="text-muted-foreground text-sm">{t("preview.loading")}</p>;

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
			<dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5">
				<dt className="text-muted-foreground">{t("preview.files")}</dt>
				<dd className="tabular-nums">{preview.files}</dd>
				<dt className="text-muted-foreground">{t("preview.conflicts")}</dt>
				<dd className="tabular-nums">{preview.conflicts}</dd>
				{preview.moved > 0 && (
					<>
						<dt className="text-muted-foreground">{t("preview.moved")}</dt>
						<dd className="tabular-nums">{preview.moved}</dd>
					</>
				)}
				{preview.winners_changed > 0 && (
					<>
						<dt className="text-muted-foreground">{t("preview.winners")}</dt>
						<dd className="tabular-nums">{preview.winners_changed}</dd>
					</>
				)}
			</dl>
			{preview.replaces && <p className="text-muted-foreground">{t("preview.replaces")}</p>}
			{lists.map(({ key, names }) =>
				names.length > 0 ? (
					<Disclosure key={key} title={t(`preview.${key}`)} count={names.length} defaultOpen={names.length <= 8}>
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

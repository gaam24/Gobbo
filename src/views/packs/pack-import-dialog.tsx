import { ExternalLink } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { commands } from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { closeThen, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { ItemActions, ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";
import { PREVIEW_KEYS } from "@/views/deploy-preview";

/// What a pack holds before it goes in: the mods already in the library, the ones inside the pack, the ones to get from
/// Nexus, and the profiles an earlier version of the pack made, with what changes in them.
export function PackImportDialog({
	gameId,
	path,
	hasNexusKey,
	onImport,
	onDownload,
	onClose,
}: {
	gameId: string;
	path: string;
	hasNexusKey: boolean;
	onImport: () => void | Promise<void>;
	/// Downloads the missing mods (a premium account), then imports.
	onDownload: () => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const { openSource } = useWorkspaceActions();
	const [failure, setFailure] = useState<string | null>(null);
	const preview = useLoad(path, () => unwrap(commands.inspectPack(gameId, path)), {
		onError: (error) => setFailure(messageOf(error)),
	});
	const fromSource = preview?.missing.filter((entry) => entry.source !== null).length ?? 0;
	const run = closeThen(onClose);

	let content: ReactNode;

	if (failure) {
		content = <FieldError>{failure}</FieldError>;
	} else if (preview === undefined) {
		content = <p className="text-muted-foreground text-sm">{t("pack.reading")}</p>;
	} else {
		content = (
			<>
				{preview.info.description && <p className="whitespace-pre-wrap">{preview.info.description}</p>}
				<p className="text-muted-foreground">
					{t("pack.summary", {
						total: preview.total,
						have: preview.have,
						bundled: preview.bundled,
						missing: preview.missing.length,
					})}
				</p>
				{preview.updates.length > 0 && (
					<BoxList>
						{preview.updates.map((update) => (
							<BoxListRow key={update.profile} className="flex-col items-stretch gap-1">
								<ItemTitle>
									{t("pack.update", {
										profile: update.profile,
										from: update.from || "?",
									})}
								</ItemTitle>
								{PREVIEW_KEYS.map((key) =>
									update[key].length > 0 ? (
										<ItemDescription key={key}>{`${t(`preview.${key}`)}: ${update[key].join(", ")}`}</ItemDescription>
									) : null,
								)}
							</BoxListRow>
						))}
					</BoxList>
				)}
				{preview.missing.length > 0 && (
					<Disclosure title={t("pack.missing")} count={preview.missing.length} defaultOpen={preview.missing.length <= 10}>
						<BoxList scroll>
							{preview.missing.map((entry) => (
								<BoxListRow key={`${entry.name}${entry.version ?? ""}`}>
									<ItemContent className="gap-0.5">
										<span className="truncate">
											{entry.name}
											{entry.version && <span className="text-subtle-foreground"> {entry.version}</span>}
										</span>
										{entry.archive && <ItemDescription className="truncate text-xs">{entry.archive}</ItemDescription>}
									</ItemContent>
									{entry.source && (
										<ItemActions>
											<Button
												variant="ghost"
												size="sm"
												onClick={() => openSource(entry.source?.provider ?? "", entry.source?.mod_id ?? "")}
											>
												<ExternalLink /> {t("pack.openPage")}
											</Button>
										</ItemActions>
									)}
								</BoxListRow>
							))}
						</BoxList>
						<p className="text-muted-foreground text-xs">{t("pack.missingHint")}</p>
					</Disclosure>
				)}
			</>
		);
	}

	return (
		<ModalDialog
			title={preview ? `${preview.info.name} ${preview.info.version}` : t("pack.importTitle")}
			description={preview?.info.author ? t("pack.by", { author: preview.info.author }) : t("pack.importHint")}
			onClose={onClose}
		>
			{content}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				{fromSource > 0 && (
					<Button variant="outline" disabled={!hasNexusKey} title={t("pack.downloadHint")} onClick={() => run(onDownload)}>
						{t("pack.download", { count: fromSource })}
					</Button>
				)}
				<Button disabled={preview === undefined || failure !== null} onClick={() => run(onImport)}>
					{t("pack.importAction")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

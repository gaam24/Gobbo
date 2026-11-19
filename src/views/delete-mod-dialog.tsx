import { useState } from "react";
import { commands, type Status } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { ItemActions, ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { type Mod, pagePartsOf } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";
import { VersionFacts } from "@/views/mod-table/cells";

/// The page a confirmation deletes, as read from one game status.
interface PageDeleteState {
	parts: Mod[];
	name: string;
	prepared: boolean;
}

/// The page a confirmation deletes: its installed parts, its name and whether an apply holds one of the parts. A
/// page-wide delete needs nothing else from the backend, so the dialog reads all of it from the status.
function pageDeleteState(status: Status, pageId: string): PageDeleteState {
	const parts = pagePartsOf(status.mods, pageId);
	const first = parts[0];
	return {
		parts,
		name: first?.page_name ?? first?.info.name ?? pageId,
		prepared: status.deployed && parts.some((part) => status.deployed_list.some((row) => row.id === part.id)),
	};
}

/// Asks before a mod, one of its versions, or a whole source page leaves the library. In mod and version mode it lists
/// the profiles that use it, and whether a prepared deployment or a running game is affected; a version that a profile
/// still selects asks for its replacement. A whole page is deleted through its first part, with every part listed.
export function DeleteModDialog({
	gameId,
	status,
	modId,
	pageId = null,
	versionFolder = null,
	onDelete,
	onDeleteVersion,
	onDeletePage,
	onClose,
	onError,
}: {
	gameId: string;
	status: Status;
	/// The mod to delete. `null` keeps the dialog closed, unless `pageId` names a page.
	modId: string | null;
	/// The source page to delete with every installed part. `null` keeps the page mode closed.
	pageId?: string | null;
	/// The installed version to delete. `null` deletes the whole mod.
	versionFolder?: string | null;
	onDelete: (modId: string) => void | Promise<void>;
	onDeleteVersion?: (modId: string, folder: string, replacement: string | null, version: string) => void | Promise<void>;
	onDeletePage?: (pageId: string) => void | Promise<void>;
	onClose: () => void;
	onError: (message: string) => void;
}) {
	const { t } = useTranslation();
	const page = pageId === null ? null : pageDeleteState(status, pageId);
	const mod = status.mods.find((item) => item.id === modId);
	const name = page?.name ?? mod?.info.name ?? modId ?? "";
	const familyFolder = mod?.folder ?? versionFolder ?? "";
	// A page needs neither the per-profile usage nor the version list: the backend deletes the whole page, and the
	// dialog reads its parts from the status. Empty keys keep both commands from running in page mode.
	const usage = useLoad(
		page !== null || modId === null ? null : `${gameId}\n${modId}`,
		() => unwrap(commands.modUsage(gameId, modId ?? "")),
		{
			onError: (failure) => onError(messageOf(failure)),
		},
	);
	const versions = useLoad(
		page !== null || modId === null || familyFolder === "" ? null : `${gameId}\n${familyFolder}`,
		() => unwrap(commands.modVersions(gameId, familyFolder)),
		{ onError: (failure) => onError(messageOf(failure)) },
	);
	const versionMode = page === null && versionFolder !== null;
	const target = versionMode ? versions?.find((version) => version.folder === versionFolder) : undefined;
	const targetName = target?.version ?? target?.folder ?? versionFolder ?? "";
	const profiles = versionMode ? (usage?.profiles.filter((profile) => profile.folder === versionFolder) ?? []) : (usage?.profiles ?? []);
	const used = profiles.length > 0;
	const others = versionMode ? (versions?.filter((version) => version.folder !== versionFolder) ?? []) : [];
	const [chosen, setChosen] = useState<string | null>(null);
	const replacement = others.find((version) => version.folder === chosen)?.folder ?? others[0]?.folder ?? null;
	const onlyVersion = versionMode && versions !== undefined && others.length === 0;
	let prepared = usage?.prepared === true;
	let running = usage?.running === true;
	let ready = usage !== undefined;
	let blocked = false;
	if (page) {
		// A page is deleted without its usage; a running game still refuses the command, and the dialog says so.
		prepared = page.prepared;
		running = false;
		ready = page.parts.length > 0;
		blocked = !onDeletePage;
	} else if (versionMode) {
		prepared = profiles.some((profile) => profile.prepared);
		running = profiles.some((profile) => profile.running);
		ready = target !== undefined;
		blocked = !onDeleteVersion || onlyVersion || (used && replacement === null);
	}
	const close = onClose;

	// The confirm button closes the dialog itself, and that close reaches `onClose` through `onOpenChange`.
	const remove = () => {
		if (pageId !== null) {
			void onDeletePage?.(pageId);
		} else if (versionFolder !== null) {
			if (modId) void onDeleteVersion?.(modId, versionFolder, used ? replacement : null, targetName);
		} else if (modId) {
			void onDelete(modId);
		}
	};

	let title = t("mods.confirmDelete.title", { name });
	let body = t("mods.confirmDelete.body", { name });
	let action = t("mods.confirmDelete.action");
	if (page) {
		title = t("mods.confirmDelete.pageTitle", { name });
		body = t("mods.confirmDelete.pageBody", { name });
		action = t("mods.confirmDelete.pageAction");
	} else if (versionMode) {
		title = t("mods.confirmDelete.versionTitle", { version: targetName, name });
		body = t("mods.confirmDelete.versionBody", { version: targetName, name });
		action = t("mods.confirmDelete.versionAction");
	}

	return (
		<ConfirmDialog
			open={modId !== null || pageId !== null}
			onOpenChange={(open) => {
				if (!open) close();
			}}
			title={title}
			confirmLabel={action}
			confirmDisabled={!ready || running || blocked}
			onConfirm={remove}
		>
			<p>{body}</p>

			{target && (
				<div className="bg-muted/50 grid gap-0.5 rounded-md border px-2.5 py-2">
					<VersionFacts version={target} />
				</div>
			)}

			{page && <PageDeleteDetails parts={page.parts} prepared={prepared} />}

			{!versionMode && versions && versions.length > 1 && (
				<section className="grid gap-2">
					<SectionHeading title={t("mods.confirmDelete.versions")} count={versions.length} />
					<BoxList scroll>
						{versions.map((version) => (
							<BoxListRow key={version.folder}>
								<ItemContent className="min-w-0 flex-row items-center justify-between gap-3">
									<ItemTitle className="min-w-0 truncate font-mono text-xs font-normal tabular-nums">
										{version.version ?? version.folder}
									</ItemTitle>
								</ItemContent>
								{version.used_by > 0 && (
									<ItemActions>
										<ItemDescription className="text-subtle-foreground text-xs">{t("mods.version.inUse")}</ItemDescription>
									</ItemActions>
								)}
							</BoxListRow>
						))}
					</BoxList>
				</section>
			)}

			{profiles.length > 0 && (
				<section className="grid gap-2">
					<SectionHeading title={t("mods.confirmDelete.usedBy")} count={profiles.length} />
					<BoxList scroll>
						{profiles.map((profile) => (
							<BoxListRow key={profile.id}>
								<ItemContent className="min-w-0 flex-row items-center justify-between gap-3">
									<ItemTitle className="min-w-0 truncate font-normal">{profile.name}</ItemTitle>
									<ItemDescription className="shrink-0 font-mono text-xs">
										{profile.version ?? t("common.none")}
										{!profile.enabled && ` · ${t("mods.state.disabled")}`}
									</ItemDescription>
								</ItemContent>
							</BoxListRow>
						))}
					</BoxList>
				</section>
			)}

			{versionMode && used && others.length > 0 && (
				<div className="grid gap-1.5">
					<span className="text-foreground text-sm font-medium">{t("mods.confirmDelete.replacement")}</span>
					<Select value={replacement ?? undefined} onValueChange={setChosen}>
						<SelectTrigger className="w-full" aria-label={t("mods.confirmDelete.replacement")}>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{others.map((version) => (
								<SelectItem key={version.folder} value={version.folder}>
									{version.version ?? version.folder}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<p className="text-muted-foreground text-xs">{t("mods.confirmDelete.replacementHint")}</p>
				</div>
			)}

			{onlyVersion && <p>{t("mods.confirmDelete.onlyVersion")}</p>}
			{prepared && page === null && <p>{t("mods.confirmDelete.prepared")}</p>}
			{running && <p className="text-destructive">{t("mods.confirmDelete.running")}</p>}
		</ConfirmDialog>
	);
}

/// The page part of the confirmation: the installed parts and the warnings for an applied page.
function PageDeleteDetails({ parts, prepared }: { parts: ReadonlyArray<Pick<Mod, "id" | "info">>; prepared: boolean }) {
	const { t } = useTranslation();
	return (
		<>
			<section className="grid gap-2">
				<SectionHeading title={t("mods.confirmDelete.pageParts")} count={parts.length} />
				<DeletePageParts parts={parts} />
			</section>
			{prepared && <p>{t("mods.confirmDelete.pagePrepared")}</p>}
			{prepared && <p>{t("mods.confirmDelete.pageRunning")}</p>}
		</>
	);
}

/// The installed parts of a page with the version each part shows, for the page confirmation. Separated from the dialog
/// so the list can be rendered without a browser.
export function DeletePageParts({ parts }: { parts: ReadonlyArray<Pick<Mod, "id" | "info">> }) {
	const { t } = useTranslation();
	return (
		<BoxList scroll>
			{parts.map((part) => (
				<BoxListRow key={part.id}>
					<ItemContent className="min-w-0 flex-row items-center justify-between gap-3">
						<ItemTitle className="min-w-0 truncate font-normal">{part.info.name}</ItemTitle>
						<ItemDescription className="text-subtle-foreground shrink-0 font-mono text-xs tabular-nums">
							{part.info.version ?? t("common.none")}
						</ItemDescription>
					</ItemContent>
				</BoxListRow>
			))}
		</BoxList>
	);
}

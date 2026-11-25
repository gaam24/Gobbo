import { EllipsisVertical, FileSearch, FolderArchive, FolderOpen, Trash2 } from "lucide-react";
import { useState } from "react";
import { useUiState } from "@/app/contexts/ui-context";
import type { ModInfoEdit, Relations, Status, UpdateReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTranslation } from "@/i18n";
import { authorOf, conflictsOf, type FullMod, type Mod, pageDescriptionOf } from "@/lib/mods";
import { ModPicture } from "@/sources/mod-picture";
import { SourceSection } from "@/sources/source-section";
import { ConflictsTab, FileWinners } from "@/views/mod-inspector/tabs/conflicts-tab";
import { FilesTab } from "@/views/mod-inspector/tabs/files-tab";
import { OverviewTab } from "@/views/mod-inspector/tabs/overview-tab";
import { useModDetails } from "@/views/mod-inspector/use-mod-details";
import { useModTags } from "@/views/mods/use-mod-tags";

type InspectorTab = "overview" | "conflicts" | "files" | "sources";

/// What the inspector does to mods. The app owns the implementations; the views only call them.
export interface ModActions {
	onSetEnabled: (ids: string[], enabled: boolean) => void | Promise<void>;
	onSelect: (folders: string[]) => void;
	onSetTags: (modId: string, tags: string[]) => void | Promise<void>;
	onSetNote: (modId: string, note: string) => void | Promise<void>;
	onSetInstructions: (modId: string, text: string) => void | Promise<void>;
	onSetRelations: (modId: string, relations: Relations) => void | Promise<void>;
	onSetFileWinner: (path: string, familyId: string | null) => void | Promise<void>;
	onMoveBeside: (modId: string, otherId: string, side: "above" | "below") => void | Promise<void>;
	onSetSource: (modId: string, provider: string, sourceId: string | null) => void | Promise<void>;
	onIgnoreVersion: (modId: string, provider: string) => void | Promise<void>;
	onUnignoreVersion: (modId: string, provider: string) => void | Promise<void>;
	onCheck: (modId: string) => Promise<UpdateReport>;
	onSelectVersion: (modId: string, folder: string) => void | Promise<void>;
	onDeleteVersion: (modId: string, folder: string) => void;
	onDeleteUnusedVersions: () => void | Promise<void>;
	onOpenFolder: (path: string) => void | Promise<void>;
	onRevealFile: (path: string) => void | Promise<void>;
	onArchiveReport: (folder: string, name: string) => void;
	onOpenGames: () => void;
	onRemoveFromLibrary: (modId: string) => void;
	onError: (message: string) => void;
	onSetInfo: (modId: string, folder: string, info: ModInfoEdit) => void | Promise<void>;
	onSetSteps: (folder: string, beforeStart: string[], afterEnd: string[]) => void | Promise<void>;
}

/// The selected mod: a header with the name, the enable switch and the tabs; the body shows the chosen tab.
export function ModDetails({
	gameId,
	status,
	mod,
	update,
	checkedAt,
	onSetEnabled,
	onSelect,
	onSetTags,
	onSetNote,
	onSetInstructions,
	onSetRelations,
	onSetFileWinner,
	onMoveBeside,
	onSetSource,
	onIgnoreVersion,
	onUnignoreVersion,
	onCheck,
	onSelectVersion,
	onDeleteVersion,
	onDeleteUnusedVersions,
	onOpenFolder,
	onRevealFile,
	onArchiveReport,
	onOpenGames,
	onRemoveFromLibrary,
	onError,
	onSetInfo,
	onSetSteps,
}: ModActions & {
	gameId: string;
	status: Status;
	mod: Mod;
	update?: UpdateReport;
	checkedAt?: number;
}) {
	const { t } = useTranslation();
	const { detailsTab, detailsRequest } = useUiState();
	const [tab, setTab] = useState<InspectorTab>(detailsTab);
	const [seenRequest, setSeenRequest] = useState(detailsRequest);
	// A click on a conflict counter asks for the conflicts tab; a plain row click asks for the overview. Adjusting the
	// state during render follows the request before the browser sees it and avoids an effect that renders twice.
	if (detailsRequest !== seenRequest) {
		setSeenRequest(detailsRequest);
		setTab(detailsTab);
	}

	const { details, integrity, verifying, verifyFiles } = useModDetails({ gameId, folder: mod.folder, status, onError });
	const loaded = details?.folder === mod.folder;
	const extras = details?.folder === mod.folder ? details.extras : null;
	// The list row carries only the fields it draws. The inspector reads the full description and page data with the
	// details, and keeps the light source until they arrive so the header never blanks.
	const detailedMod: FullMod = {
		...mod,
		info: details?.folder === mod.folder ? details.info : mod.info,
		source: extras?.source ?? (mod.source ? { ...mod.source, tags: [] } : null),
		page_description: details?.folder === mod.folder ? pageDescriptionOf(details.sources, extras?.source?.description) : null,
		notes: extras?.notes ?? "",
		requirements: extras?.requirements ?? [],
		source_dlcs: extras?.source_dlcs ?? [],
		source_sync: extras?.source_sync ?? [],
		fomod: extras?.fomod ?? [],
		fomod_installer: extras?.fomod_installer ?? false,
		requires: extras?.requires ?? [],
		incompatible: extras?.incompatible ?? [],
		incompatible_names: extras?.incompatible_names ?? [],
		overrides: extras?.overrides ?? [],
		overrides_names: extras?.overrides_names ?? [],
		steps_before: extras?.steps_before ?? [],
		steps_after: extras?.steps_after ?? [],
	};
	const conflicts =
		details?.folder === mod.folder ? conflictsOf(details.conflicts, mod.id) : new Map<string, { win: number; lose: number }>();
	const allTags = useModTags(status.mods);
	const author = authorOf(mod);

	const toggleEnabled = () => void onSetEnabled([mod.id], !mod.enabled);
	const changeTab = (value: string) => {
		if (value === "overview" || value === "conflicts" || value === "files" || value === "sources") setTab(value);
	};

	const tabs: Array<{ id: InspectorTab; label: string; count?: number }> = [
		{ id: "overview", label: t("mods.tabs.overview") },
		{ id: "conflicts", label: t("mods.tabs.conflicts"), count: conflicts.size },
		{ id: "files", label: t("mods.tabs.files"), count: mod.files },
		{ id: "sources", label: t("mods.tabs.sources") },
	];

	return (
		<Tabs value={tab} onValueChange={changeTab} className="grid content-start">
			<header className="grid gap-4 border-b px-5 pt-5">
				{mod.source?.picture && (
					<ModPicture
						gameId={gameId}
						provider={mod.source.provider}
						file={mod.source.picture}
						url={mod.source.picture_url ?? null}
						name={mod.info.name}
						adult={mod.source.adult}
					/>
				)}
				<div className="flex items-start gap-3">
					<div className="grid min-w-0 flex-1 gap-1">
						<h2 className="text-lg leading-tight font-semibold wrap-break-word">{mod.info.name}</h2>
						{author && <p className="text-muted-foreground text-sm">{t("mods.byAuthor", { author })}</p>}
						{detailedMod.source?.summary && <p className="text-muted-foreground mt-1 text-sm">{detailedMod.source.summary}</p>}
					</div>
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button variant="outline" size="icon" aria-label={t("mods.more")} title={t("mods.more")}>
								<EllipsisVertical />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent className="w-56">
							<DropdownMenuItem onSelect={() => void onOpenFolder(`${status.library_dir}/${mod.folder}`)}>
								<FolderOpen /> {t("mods.openFolder")}
							</DropdownMenuItem>
							<DropdownMenuItem
								disabled={!mod.archive_file}
								onSelect={() => mod.archive_file && void onRevealFile(`${status.archives_dir}/${mod.archive_file}`)}
							>
								<FolderArchive /> {t("mods.showArchive")}
							</DropdownMenuItem>
							<DropdownMenuItem disabled={!mod.archive_file} onSelect={() => onArchiveReport(mod.folder, mod.info.name)}>
								<FileSearch /> {t("mods.archiveReport")}
							</DropdownMenuItem>
							<DropdownMenuSeparator />
							<DropdownMenuItem variant="destructive" onSelect={() => onRemoveFromLibrary(mod.id)}>
								<Trash2 /> {t("mods.deleteMod")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</div>

				<div className="bg-muted flex items-center justify-between gap-3 rounded-lg px-3.5 py-3">
					<div className="grid gap-0.5">
						<span className="font-medium">{t("mods.enabledHere")}</span>
						<span className="text-muted-foreground text-xs">{t("mods.enabledHint")}</span>
					</div>
					<Switch
						checked={mod.enabled}
						size="default"
						aria-label={t("mods.toggle", { name: mod.info.name })}
						className="-my-2.5 -mr-1"
						onCheckedChange={toggleEnabled}
					/>
				</div>

				<TabsList variant="line" aria-label={t("mods.inspectorTabs")} className="w-full justify-start gap-4">
					{tabs.map((item) => (
						<TabsTrigger key={item.id} value={item.id} className="flex-none">
							{item.label}
							{item.count !== undefined && item.count > 0 && (
								<span className="text-subtle-foreground ml-2 font-mono text-xs tabular-nums">{item.count}</span>
							)}
						</TabsTrigger>
					))}
				</TabsList>
			</header>

			<TabsContent value="overview" className="grid content-start gap-6 p-5">
				<OverviewTab
					mod={detailedMod}
					versions={details?.folder === mod.folder ? details.versions : []}
					update={update}
					allTags={allTags}
					onSelectVersion={onSelectVersion}
					onDeleteVersion={onDeleteVersion}
					onDeleteUnusedVersions={onDeleteUnusedVersions}
					onSetTags={onSetTags}
					onSetNote={onSetNote}
					onSetInstructions={onSetInstructions}
					onSetRelations={onSetRelations}
					mods={status.mods}
					onSetInfo={onSetInfo}
					onSetSteps={onSetSteps}
					steps={details?.folder === mod.folder ? details.steps : null}
					onRemove={onRemoveFromLibrary}
					onIgnoreVersion={onIgnoreVersion}
					onUnignoreVersion={onUnignoreVersion}
				/>
			</TabsContent>

			<TabsContent value="conflicts" className="grid content-start gap-6 p-5">
				<ConflictsTab modId={mod.id} conflicts={conflicts} status={status} onSelect={onSelect} onMoveBeside={onMoveBeside} />
				<FileWinners
					modId={mod.id}
					conflicts={details?.folder === mod.folder ? details.conflicts : []}
					status={status}
					canChoose={details?.folder === mod.folder && details.file_winners}
					onSetWinner={onSetFileWinner}
				/>
			</TabsContent>

			<TabsContent value="files" className="grid content-start gap-6 p-5">
				<FilesTab
					files={loaded ? details.files : null}
					count={mod.files}
					skipped={mod.skipped}
					integrity={integrity?.folder === mod.folder ? integrity.status : null}
					verifying={verifying}
					onVerify={verifyFiles}
					onArchiveReport={() => onArchiveReport(mod.folder, mod.info.name)}
				/>
			</TabsContent>

			<TabsContent value="sources" className="grid content-start gap-6 p-5">
				<SourceSection
					key={`${mod.folder}-${mod.sources.map((source) => `${source.provider}:${source.mod_id}`).join("|")}`}
					mod={detailedMod}
					update={update}
					checkedAt={checkedAt}
					onSetSource={onSetSource}
					onCheck={onCheck}
					onIgnore={onIgnoreVersion}
					onUnignore={onUnignoreVersion}
					onOpenGames={onOpenGames}
					onError={onError}
				/>
			</TabsContent>
		</Tabs>
	);
}

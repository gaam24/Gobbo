import { Check, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import type { UpdateReport, VersionView } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { ItemActions, ItemContent } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import type { FullMod } from "@/lib/mods";
import { UpdateLine } from "@/sources/update-line";
import { VersionFacts } from "@/views/mod-table/cells";

/// The version as the list shows it: its number, or its folder when the archive or `info.json` gives no number.
function versionName(version: VersionView): string {
	return version.version ?? version.folder;
}

/// Every installed version of the selected mod. The current one is marked, and each other one can be used or deleted
/// without leaving the inspector. A version that a profile still selects asks for its replacement in the delete dialog.
export function VersionSection({
	mod,
	versions,
	update,
	onSelectVersion,
	onDeleteVersion,
	onDeleteUnusedVersions,
	onIgnoreVersion,
	onUnignoreVersion,
}: {
	mod: FullMod;
	versions: VersionView[];
	update?: UpdateReport;
	onSelectVersion: (modId: string, folder: string) => void | Promise<void>;
	onDeleteVersion: (modId: string, folder: string) => void;
	onDeleteUnusedVersions: () => void | Promise<void>;
	onIgnoreVersion: (modId: string, provider: string) => void | Promise<void>;
	onUnignoreVersion: (modId: string, provider: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const { openSource } = useWorkspaceActions();
	const source = mod.source;
	const openUpdate =
		source && update?.has_update && update.latest && !update.latest_installed
			? () => openSource(source.provider, source.mod_id, "files")
			: undefined;
	/// The backend names the folder of the installed latest version; selecting it needs no list.
	const installedFolder = update?.installed_folder;
	const useInstalled = installedFolder ? () => void onSelectVersion(mod.id, installedFolder) : undefined;
	const ignoreUpdate = update?.latest && update.has_update ? () => void onIgnoreVersion(mod.id, update.provider) : undefined;
	const showUpdate = update?.state === "ignored" ? () => void onUnignoreVersion(mod.id, update.provider) : undefined;
	const hasUnused = versions.some((version) => version.used_by === 0);

	let body: ReactNode;

	if (mod.version_count <= 1) {
		body = <span className="font-mono text-sm tabular-nums">{mod.info.version ?? t("common.none")}</span>;
	} else if (versions.length === 0) {
		body = <p className="text-muted-foreground text-sm">{t("mods.version.loading")}</p>;
	} else {
		body = (
			<BoxList scroll>
				{versions.map((version) => {
					const current = version.folder === mod.folder;
					return (
						<BoxListRow key={version.folder} className="data-[current=true]:bg-muted/50 items-start" data-current={current}>
							<Check className={current ? "text-primary mt-0.5 size-3.5 shrink-0" : "mt-0.5 size-3.5 shrink-0 opacity-0"} aria-hidden />
							<ItemContent className="min-w-0 gap-0.5">
								<div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
									<span className="font-mono text-sm tabular-nums">{versionName(version)}</span>
									{current && <span className="text-primary text-xs font-medium">{t("mods.version.current")}</span>}
									{version.used_by > 0 && <span className="text-subtle-foreground text-xs">{t("mods.version.inUse")}</span>}
								</div>
								<VersionFacts version={version} />
							</ItemContent>
							<ItemActions className="shrink-0 gap-0.5">
								{!current && (
									<Button variant="ghost" size="sm" className="h-7" onClick={() => void onSelectVersion(mod.id, version.folder)}>
										{t("mods.version.use")}
									</Button>
								)}
								<Button
									variant="ghost"
									size="icon-sm"
									aria-label={t("mods.version.delete", {
										version: versionName(version),
									})}
									title={t("mods.version.delete", {
										version: versionName(version),
									})}
									onClick={() => onDeleteVersion(mod.id, version.folder)}
								>
									<Trash2 />
								</Button>
							</ItemActions>
						</BoxListRow>
					);
				})}
			</BoxList>
		);
	}

	return (
		<section className="grid gap-2">
			<SectionHeading
				title={t("mods.info.version")}
				actions={
					mod.version_count > 1 && hasUnused ? (
						<Button
							variant="ghost"
							size="sm"
							className="text-muted-foreground hover:text-foreground h-7"
							title={t("mods.deleteUnusedVersionsHint")}
							onClick={() => void onDeleteUnusedVersions()}
						>
							<Trash2 /> {t("mods.deleteUnusedVersions")}
						</Button>
					) : undefined
				}
			/>

			{body}

			<UpdateLine update={update} onOpen={openUpdate} onUse={useInstalled} onIgnore={ignoreUpdate} onUnignore={showUpdate} />
		</section>
	);
}

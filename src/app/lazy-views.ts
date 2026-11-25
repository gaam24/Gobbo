import { lazy } from "react";

export const ModInspector = lazy(async () => {
	const { ModInspector: component } = await import("@/views/mod-inspector");
	return { default: component };
});

// Screens the user opens on demand load as their own chunks, so the first paint parses only the main screen.
export const GamesPanel = lazy(async () => {
	const { GamesPanel: component } = await import("@/views/games/games-panel");
	return { default: component };
});

export const GameEditor = lazy(async () => {
	const { GameEditor: component } = await import("@/views/games/editor/game-editor");
	return { default: component };
});

export const DefinitionImportDialog = lazy(async () => {
	const { DefinitionImportDialog: component } = await import("@/views/games/import-definition-dialog");
	return { default: component };
});

export const CatalogDialog = lazy(async () => {
	const { CatalogDialog: component } = await import("@/views/games/catalog-dialog");
	return { default: component };
});

export const AppSettings = lazy(async () => {
	const { AppSettings: component } = await import("@/views/settings/app-settings");
	return { default: component };
});

export const StorageView = lazy(async () => {
	const { StorageView: component } = await import("@/views/storage/storage-page");
	return { default: component };
});

export const GameSettings = lazy(async () => {
	const { GameSettings: component } = await import("@/views/settings/game/game-settings");
	return { default: component };
});

export const FomodWizard = lazy(async () => {
	const { FomodWizard: component } = await import("@/views/fomod/fomod-wizard");
	return { default: component };
});

export const ReinstallDialog = lazy(async () => {
	const { ReinstallDialog: component } = await import("@/views/fomod/reinstall-dialog");
	return { default: component };
});

export const DeployPreviewDialog = lazy(async () => {
	const { DeployPreviewDialog: component } = await import("@/views/deploy-preview");
	return { default: component };
});

export const CommunityProfileDialog = lazy(async () => {
	const { CommunityProfileDialog: component } = await import("@/views/mods/community-profile-dialog");
	return { default: component };
});

export const SortPreviewDialog = lazy(async () => {
	const { SortPreviewDialog: component } = await import("@/views/mods/sort-preview-dialog");
	return { default: component };
});

export const LoadOrderDialog = lazy(async () => {
	const { LoadOrderDialog: component } = await import("@/views/load-order/load-order-dialog");
	return { default: component };
});

export const ManualUpdateDialog = lazy(async () => {
	const { ManualUpdateDialog: component } = await import("@/views/mods/manual-update-dialog");
	return { default: component };
});

export const ConflictExplorerDialog = lazy(async () => {
	const { ConflictExplorerDialog: component } = await import("@/views/conflict-explorer");
	return { default: component };
});

export const SnapshotDialog = lazy(async () => {
	const { SnapshotDialog: component } = await import("@/views/mods/snapshot-dialog");
	return { default: component };
});

export const ArchiveReportDialog = lazy(async () => {
	const { ArchiveReportDialog: component } = await import("@/views/mods/archive-report-dialog");
	return { default: component };
});

export const HealthDialog = lazy(async () => {
	const { HealthDialog: component } = await import("@/views/mods/health-dialog");
	return { default: component };
});

export const SetupDialog = lazy(async () => {
	const { SetupDialog: component } = await import("@/views/setup/setup-dialog");
	return { default: component };
});

export const PackExportDialog = lazy(async () => {
	const { PackExportDialog: component } = await import("@/views/packs/pack-export-dialog");
	return { default: component };
});

export const PackLinkDialog = lazy(async () => {
	const { PackLinkDialog: component } = await import("@/views/packs/pack-link-dialog");
	return { default: component };
});

export const CollectionDialog = lazy(async () => {
	const { CollectionDialog: component } = await import("@/views/collections/collection-dialog");
	return { default: component };
});

export const PackImportDialog = lazy(async () => {
	const { PackImportDialog: component } = await import("@/views/packs/pack-import-dialog");
	return { default: component };
});

export const OverwriteDialog = lazy(async () => {
	const { OverwriteDialog: component } = await import("@/views/overwrite-dialog");
	return { default: component };
});

export const DownloadsView = lazy(async () => {
	const { DownloadsView: component } = await import("@/nexus/downloads-view");
	return { default: component };
});

export const NotificationsView = lazy(async () => {
	const { NotificationsView: component } = await import("@/views/notifications-view");
	return { default: component };
});

export const CommandPalette = lazy(async () => {
	const { CommandPalette: component } = await import("@/views/command-palette");
	return { default: component };
});

export const ShortcutsDialog = lazy(async () => {
	const { ShortcutsDialog: component } = await import("@/views/shortcuts-dialog");
	return { default: component };
});

export const DeleteModDialog = lazy(async () => {
	const { DeleteModDialog: component } = await import("@/views/delete-mod-dialog");
	return { default: component };
});

export const GroupEditDialog = lazy(async () => {
	const { GroupEditDialog: component } = await import("@/views/groups/group-edit-dialog");
	return { default: component };
});

export const TranslationWinnersDialog = lazy(async () => {
	const { TranslationWinnersDialog: component } = await import("@/views/groups/translation-winners-dialog");
	return { default: component };
});

export const TranslationHintsDialog = lazy(async () => {
	const { TranslationHintsDialog: component } = await import("@/views/groups/translation-hints-dialog");
	return { default: component };
});

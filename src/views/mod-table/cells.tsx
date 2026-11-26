import { cn } from "cn";
import { Check, ChevronDown, CircleArrowUp, Download, EyeOff, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import type { UpdateReport, VersionView } from "@/bindings";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { useTranslation } from "@/i18n";
import { activeConflicts, checkedAtOf, formatCount, formatDate, formatSize, type Mod } from "@/lib/mods";
import { EMPTY } from "@/views/mod-table/layout";
import { useModVersions } from "@/views/mod-table/use-mod-versions";

/// The version and its update state, with a picker when more than one version is installed. The arrow and the words
/// say "update available", not only the colour.
export function VersionCell({
	gameId,
	mod,
	update,
	onSelectVersion,
	onDeleteVersion,
	onUpdate,
}: {
	gameId: string;
	mod: Mod;
	update?: UpdateReport;
	onSelectVersion: (modId: string, folder: string) => void | Promise<void>;
	/// Opens the confirmation for deleting an installed version.
	onDeleteVersion?: (modId: string, folder: string) => void;
	/// Downloads the newest provider file and installs it as the selected version.
	onUpdate?: () => void | Promise<void>;
}) {
	const { t, i18n } = useTranslation();
	const installed = mod.info.version ?? t("common.none");
	/// The time of the saved check, shown on hover: the state above is the result of that check.
	const checkedAt = checkedAtOf(mod);
	const { versions, failed, loadVersions } = useModVersions(gameId, mod.folder);

	/// The provider's latest version is in the library already. The backend names its folder, so it becomes the selected
	/// one and the user does not have to open the picker to understand that "Installed 1.4" means "1.4 is ready to use".
	const selectInstalled = async () => {
		const folder = update?.installed_folder;

		if (folder) await onSelectVersion(mod.id, folder);
	};

	let line: ReactNode = null;

	if (update?.error) {
		line = <span className="text-subtle-foreground">{t("mods.version.checkFailed")}</span>;
	} else if (update?.has_update && update.latest) {
		line = update.latest_installed ? (
			<Button
				variant="link"
				title={t("mods.version.useInstalled", { version: update.latest })}
				aria-label={t("mods.version.useInstalled", { version: update.latest })}
				className="text-muted-foreground h-auto justify-start gap-1 p-0 text-sm font-normal"
				onClick={() => void selectInstalled()}
			>
				<Check className="size-3.5 shrink-0" aria-hidden />
				{t("mods.version.installed", { version: update.latest })}
			</Button>
		) : (
			<UpdateLink version={update.latest} onUpdate={onUpdate} />
		);
	} else if (update?.state === "ignored") {
		line = (
			<span className="text-subtle-foreground flex items-center gap-1">
				<EyeOff className="size-3.5 shrink-0" aria-hidden />
				{t("mods.version.ignoredShort")}
			</span>
		);
	}
	// "Up to date", "Not checked" and "No source" are the normal case and were repeated under every row. The cell now
	// speaks only when a version needs attention; the tooltip still says when it was checked.

	const picker =
		mod.version_count > 1 ? (
			<DropdownMenu onOpenChange={(open) => open && void loadVersions()}>
				<DropdownMenuTrigger asChild>
					<Button
						variant="ghost"
						size="sm"
						className="-ml-2 h-6 max-w-full justify-start gap-1 px-2 font-normal"
						aria-label={t("mods.version.choose", { name: mod.info.name })}
					>
						<span className="truncate font-mono text-sm tabular-nums">{installed}</span>
						<ChevronDown className="text-muted-foreground size-4 shrink-0" aria-hidden />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="start" className="w-80">
					<DropdownMenuLabel>{t("mods.col.versions")}</DropdownMenuLabel>
					{failed && <DropdownMenuItem disabled>{t("mods.version.loadFailed")}</DropdownMenuItem>}
					{versions === null && !failed && <DropdownMenuItem disabled>{t("mods.version.loading")}</DropdownMenuItem>}
					{versions?.map((version) => (
						<VersionItem
							key={version.folder}
							version={version}
							selected={version.folder === mod.folder}
							onSelect={() => void onSelectVersion(mod.id, version.folder)}
							onDelete={onDeleteVersion ? () => onDeleteVersion(mod.id, version.folder) : undefined}
						/>
					))}
				</DropdownMenuContent>
			</DropdownMenu>
		) : (
			<span className="truncate font-mono text-sm tabular-nums">{installed}</span>
		);

	return (
		<div
			className="grid min-w-0 gap-0.5 text-xs"
			title={
				checkedAt === undefined
					? undefined
					: t("mods.sources.checkedOn", {
							date: formatDate(checkedAt, i18n.language),
						})
			}
		>
			{picker}
			{line}
		</div>
	);
}

/// The "Available x" line: a plain label without a download, otherwise a button that downloads and installs the update.
function UpdateLink({ version, onUpdate }: { version: string; onUpdate?: () => void | Promise<void> }) {
	const { t } = useTranslation();
	const content = (
		<>
			<CircleArrowUp className="size-3.5 shrink-0" aria-hidden />
			<span className="truncate">{t("mods.version.updateAvailable", { version })}</span>
			{onUpdate && <Download className="size-3 opacity-70" aria-hidden />}
		</>
	);

	if (!onUpdate) return <span className="text-update flex max-w-full min-w-0 items-center gap-1 font-medium">{content}</span>;
	return (
		<Button
			variant="link"
			title={t("mods.version.updateNow", { version })}
			aria-label={t("mods.version.updateNow", { version })}
			className="text-update h-auto max-w-full min-w-0 justify-start gap-1 p-0 font-medium"
			onClick={() => void onUpdate()}
		>
			{content}
		</Button>
	);
}

/// Conflicts of an enabled mod, or of the parts of a page: what it wins, what it loses, or both.
export function ConflictCell({ mod, onOpen }: { mod: Mod; onOpen?: () => void }) {
	const { wins, loses } = activeConflicts(mod);
	return <ConflictText wins={wins} loses={loses} onOpen={onOpen} />;
}

/// The wins and losses as words. Text only, with the colour as a second signal; nothing to say shows a dash. With
/// `onOpen`, the count is a button that opens the conflicts tab of the inspector.
export function ConflictText({ wins, loses, onOpen }: { wins: number; loses: number; onOpen?: () => void }) {
	const { t } = useTranslation();
	let content: ReactNode;

	if (wins > 0 && loses > 0) {
		content = <span className="text-muted-foreground text-xs">{t("mods.conflictCell.both", { wins, loses })}</span>;
	} else if (loses > 0) {
		content = <span className="text-lose text-xs">{t("mods.conflictCell.loses", { count: loses })}</span>;
	} else if (wins > 0) {
		content = <span className="text-win text-xs">{t("mods.conflictCell.wins", { count: wins })}</span>;
	} else {
		content = <span className="text-subtle-foreground text-xs">{EMPTY}</span>;
	}

	if (!onOpen || (wins === 0 && loses === 0)) return content;
	return (
		<Button
			variant="link"
			className="h-auto justify-start p-0 font-normal"
			title={t("mods.conflictCell.open")}
			aria-label={t("mods.conflictCell.open")}
			onClick={onOpen}
		>
			{content}
		</Button>
	);
}

/// A date from Unix seconds, or nothing when the page or the check does not give one.
export function DateCell({ seconds, locale }: { seconds: number | null | undefined; locale: string }) {
	if (seconds == null) return <span className="text-subtle-foreground text-xs">{EMPTY}</span>;
	return <span className="text-muted-foreground text-xs">{formatDate(seconds * 1000, locale)}</span>;
}

/// A count from the provider page at the right edge of its column, or nothing when the page does not give it.
export function CountCell({ value, locale }: { value: number | null | undefined; locale: string }) {
	return <span className="text-right font-mono text-xs tabular-nums">{value == null ? EMPTY : formatCount(value, locale)}</span>;
}

/// One installed version in a version picker: a check on the selected one, the version, whether a profile uses it, its
/// facts, and its own delete action. The table's picker draws it; the inspector shares `VersionFacts`.
export function VersionItem({
	version,
	selected,
	onSelect,
	onDelete,
}: {
	version: VersionView;
	selected: boolean;
	onSelect: () => void;
	onDelete?: () => void;
}) {
	const { t } = useTranslation();
	const name = version.version ?? version.folder;
	const deleteLabel = t("mods.version.delete", { version: name });
	return (
		<div className="flex gap-0.5" role="none">
			<DropdownMenuItem className="min-w-0 flex-1 items-start" onSelect={onSelect}>
				<span className="grid min-w-0 flex-1 gap-0.5">
					<span className="flex items-center gap-2">
						<Check className={cn("size-3.5 shrink-0", selected ? "opacity-100" : "opacity-0")} aria-hidden />
						<span className="font-mono text-sm tabular-nums">{name}</span>
						{version.used_by > 0 && <span className="text-subtle-foreground ml-auto text-xs">{t("mods.version.inUse")}</span>}
					</span>
					<VersionFacts version={version} />
				</span>
			</DropdownMenuItem>
			{onDelete && (
				<DropdownMenuItem
					variant="destructive"
					className="w-8 shrink-0 items-center justify-center px-0"
					aria-label={deleteLabel}
					title={deleteLabel}
					onSelect={onDelete}
				>
					<Trash2 />
				</DropdownMenuItem>
			)}
		</div>
	);
}

/// What the picker shows under an installed version: its name, its files, its size and its install date, then its folder
/// and the archives it came from.
export function VersionFacts({ version }: { version: VersionView }) {
	const { t, i18n } = useTranslation();
	const facts = t("mods.version.facts", {
		name: version.name,
		count: version.files,
		size: formatSize(version.size_kb, i18n.language),
		date: formatDate(version.installed_at * 1000, i18n.language),
	});
	const paths = [version.folder, version.archive, version.archive_file].filter(Boolean).join(" · ");
	const shortPaths = [version.folder, version.archive_file ?? version.archive].filter(Boolean).join(" · ");
	return (
		<span className="text-subtle-foreground grid min-w-0 gap-0.5 text-xs">
			<span className="truncate" title={facts}>
				{facts}
			</span>
			<span className="truncate font-mono" title={paths}>
				{shortPaths}
			</span>
		</span>
	);
}

import { Download as DownloadIcon, Link2Off, RotateCw, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { GameView } from "@/bindings";
import { ListRow, PageEmpty } from "@/components/app/page-layout";
import { StatusBadge, type StatusTone } from "@/components/app/status-badge";
import { GameIcon } from "@/components/game-icon";
import { PageFrame } from "@/components/page-frame";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { errorText } from "@/lib/ipc";
import { formatDuration, formatSize } from "@/lib/mods";
import type { Download } from "@/sources/downloads";

interface DownloadsViewProps {
	downloads: Download[];
	games: GameView[];
	locale: string;
	/// Whether Gobbo takes the download links of any provider; without them nothing comes here.
	linksOn: boolean;
	hasNexusKey: boolean;
	/// Ids of the games added to Gobbo. A download for a removed game cannot show its mod list.
	managedIds: string[];
	onInstall: (download: Download) => void | Promise<void>;
	onRemove: (id: number) => void | Promise<void>;
	onRetry: (id: number) => void | Promise<void>;
	onClear: () => void | Promise<void>;
	onShowMods: (game: string) => void;
	onTurnOnLinks: () => void | Promise<void>;
	onOpenSettings: () => void;
}

/// The files from the "Mod Manager Download" button on Nexus: what comes in, what waits for its install, and what this
/// run installed. A download installs itself when it is done; one from an earlier run waits for the Install button.
export function DownloadsView({
	downloads,
	games,
	locale,
	linksOn,
	hasNexusKey,
	managedIds,
	onInstall,
	onRemove,
	onRetry,
	onClear,
	onShowMods,
	onTurnOnLinks,
	onOpenSettings,
}: DownloadsViewProps) {
	const { t } = useTranslation();
	// A progress event re-renders every row; resolve the game name and the managed flag once, not once per row.
	const gameNames = useMemo(() => new Map(games.map((game) => [game.id, game.name])), [games]);
	const managed = useMemo(() => new Set(managedIds), [managedIds]);
	const finished = downloads.some((download) => download.state === "installed" || download.state === "failed");
	const newestFirst = downloads.toReversed();

	return (
		<PageFrame
			title={t("downloads.title")}
			hint={t("downloads.hint")}
			action={
				finished && (
					<Button variant="outline" size="sm" onClick={() => void onClear()}>
						{t("downloads.clear")}
					</Button>
				)
			}
		>
			{!linksOn && (
				<Alert className="mx-6 mt-4">
					<Link2Off />
					<AlertTitle>{t("downloads.linksOff")}</AlertTitle>
					<AlertDescription className="grid gap-2">
						<span>{t("downloads.linksOffHint")}</span>
						<Button size="sm" className="justify-self-start" onClick={() => void onTurnOnLinks()}>
							{t("downloads.turnOn")}
						</Button>
					</AlertDescription>
				</Alert>
			)}
			{!hasNexusKey && (
				<Alert className="mx-6 mt-4">
					<AlertTitle>{t("downloads.keyMissing")}</AlertTitle>
					<AlertDescription>
						<Button size="sm" variant="outline" className="mt-2" onClick={onOpenSettings}>
							{t("downloads.openSettings")}
						</Button>
					</AlertDescription>
				</Alert>
			)}

			{downloads.length === 0 ? (
				<PageEmpty icon={DownloadIcon} title={t("downloads.empty")} description={t("downloads.emptyHint")} />
			) : (
				<ul className={!(linksOn && hasNexusKey) ? "mt-4" : undefined}>
					{newestFirst.map((download) => (
						<DownloadRow
							key={download.id}
							download={download}
							gameName={gameNames.get(download.game) ?? ""}
							showMods={managed.has(download.game)}
							locale={locale}
							onInstall={onInstall}
							onRemove={onRemove}
							onRetry={onRetry}
							onShowMods={onShowMods}
						/>
					))}
				</ul>
			)}
		</PageFrame>
	);
}

interface DownloadRowProps {
	download: Download;
	gameName: string;
	/// The game is still added, so its mod list exists.
	showMods: boolean;
	locale: string;
	onInstall: (download: Download) => void | Promise<void>;
	onRemove: (id: number) => void | Promise<void>;
	onRetry: (id: number) => void | Promise<void>;
	onShowMods: (game: string) => void;
}

const REMOVE_LABEL = {
	downloading: "downloads.cancel",
	ready: "downloads.delete",
	installing: null,
	installed: "downloads.dismiss",
	failed: "downloads.dismiss",
} as const;

const STATE_TONE = {
	downloading: "info",
	ready: "warning",
	installing: "info",
	installed: "success",
	failed: "danger",
} as const satisfies Record<Download["state"], StatusTone>;

function DownloadRow({ download, gameName, showMods, locale, onInstall, onRemove, onRetry, onShowMods }: DownloadRowProps) {
	const { t } = useTranslation();
	const { state, received_kb: received, total_kb: total, speed_kb: speed } = download;
	const [installing, setInstalling] = useState(false);

	const install = async () => {
		if (installing) return;
		setInstalling(true);

		try {
			await onInstall(download);
		} catch {
			// The provider reports the failure; the button must become usable again.
		}

		setInstalling(false);
	};
	const percent = total > 0 ? Math.min(100, Math.floor((received * 100) / total)) : null;
	const removeLabel = REMOVE_LABEL[state];
	const details = (() => {
		if (state === "downloading") {
			if (received === 0) return t("downloads.waiting");
			const parts = [
				total > 0
					? t("downloads.progress", {
							received: formatSize(received, locale),
							total: formatSize(total, locale),
						})
					: formatSize(received, locale),
			];

			if (speed > 0) {
				parts.push(t("downloads.speed", { speed: formatSize(speed, locale) }));

				if (total > received)
					parts.push(
						t("downloads.left", {
							time: formatDuration((total - received) / speed, locale),
						}),
					);
			}

			return parts.join(" · ");
		}

		return total > 0 ? formatSize(total, locale) : "";
	})();

	return (
		<ListRow asChild className="items-center gap-3 py-3">
			<li>
				<ItemMedia className="size-8">
					{download.game ? (
						<GameIcon id={download.game} name={gameName} className="size-8 rounded-md" />
					) : (
						<span className="bg-muted grid size-8 place-items-center rounded-md">
							<DownloadIcon className="text-subtle-foreground size-4" aria-hidden />
						</span>
					)}
				</ItemMedia>
				<ItemContent className="gap-1">
					<ItemTitle className="w-full" title={download.name}>
						{download.name}
					</ItemTitle>
					<div className="flex flex-wrap items-center gap-2">
						<StatusBadge tone={STATE_TONE[state]} dot>
							{t(`downloads.state.${state}`)}
						</StatusBadge>
						{state === "installing" && <Spinner aria-hidden className="size-3.5" />}
						{(gameName || details) && (
							<ItemDescription className="line-clamp-none text-xs">
								{gameName}
								{gameName && details && " · "}
								{details && <span className="font-mono tabular-nums">{details}</span>}
							</ItemDescription>
						)}
					</div>
					{state === "downloading" && <Progress value={percent ?? 0} aria-label={download.name} />}
					{download.error && <p className="text-destructive text-xs whitespace-pre-line">{errorText(download.error)}</p>}
				</ItemContent>
				<ItemActions>
					{state === "ready" && (
						<Button size="sm" disabled={installing} aria-busy={installing} onClick={() => void install()}>
							{t("downloads.install")}
						</Button>
					)}
					{state === "installed" && download.game && showMods && (
						<Button size="sm" variant="outline" onClick={() => onShowMods(download.game)}>
							{t("downloads.showMods")}
						</Button>
					)}
					{state === "failed" && (
						<Button size="sm" variant="outline" onClick={() => void onRetry(download.id)}>
							<RotateCw /> {t("common.retry")}
						</Button>
					)}
					{removeLabel && (
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={t(removeLabel)}
							title={t(removeLabel)}
							onClick={() => void onRemove(download.id)}
						>
							<X aria-hidden />
						</Button>
					)}
				</ItemActions>
			</li>
		</ListRow>
	);
}

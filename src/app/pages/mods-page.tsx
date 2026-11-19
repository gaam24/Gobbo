import { useGames } from "@/app/contexts/games-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { StatusError } from "@/components/app/status-error";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { ModsLoading } from "@/views/mods/mods-loading";
import { ModsView } from "@/views/mods/mods-view";

/// The mod list of the active game: the states before there is a list (no game, a failed or a pending status), then the
/// screen. The screen and its header and toolbar read the workspace themselves; the rows below them stay props-driven.
export function ModsPage() {
	const { t } = useTranslation();
	const { managed, catalogError, reloadGames } = useGames();
	const { gameId, status, statusError, retryStatus, showGames } = useWorkspace();

	if (managed === null) {
		if (!catalogError) return <ModsLoading />;
		return (
			<div role="alert" className="mx-auto grid max-w-md gap-3 p-10 text-center">
				<h1 className="text-lg font-semibold">{t("games.catalogFailed")}</h1>
				<p className="text-muted-foreground text-sm">{catalogError}</p>
				<Button className="justify-self-center" onClick={() => void reloadGames()}>
					{t("common.retry")}
				</Button>
			</div>
		);
	}
	if (!gameId) {
		return (
			<div className="mx-auto flex max-w-md flex-col items-center gap-4 p-10 text-center">
				<h1 className="text-lg font-semibold">{t("games.noneAdded")}</h1>
				<p className="text-muted-foreground text-sm">{t("games.emptyHint")}</p>
				<Button onClick={showGames}>{t("games.goToGames")}</Button>
			</div>
		);
	}
	if (!status) {
		if (statusError) return <StatusError error={statusError} onRetry={retryStatus} />;
		return <ModsLoading />;
	}

	return <ModsView key={`${gameId}:${status.profiles.find((profile) => profile.active)?.id ?? "none"}`} gameId={gameId} status={status} />;
}

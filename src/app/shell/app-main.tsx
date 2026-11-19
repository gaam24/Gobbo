import { useNavigation } from "@/app/contexts/navigation-context";
import { AppSettingsPage } from "@/app/pages/app-settings-page";
import { DownloadsPage } from "@/app/pages/downloads-page";
import { GameSettingsPage } from "@/app/pages/game-settings-page";
import { GamesPage } from "@/app/pages/games-page";
import { ModsPage } from "@/app/pages/mods-page";
import { NotificationsPage } from "@/app/pages/notifications-page";
import { StoragePage } from "@/app/pages/storage-page";
import type { AppView } from "@/app/prefs";
import { ErrorBoundary } from "@/components/app/error-boundary";
import { assertNever } from "@/lib/assert";
import { e2eForcedFailure } from "@/lib/e2e-bridge";

/// The main column. Each screen draws its own header with `PageHeader`, which also offers the way back. A view is a screen, not a tab that keeps its state; the data lives in the providers above and the
/// pages only compose it. The switch is exhaustive, so a new view fails to compile until it has a page.
/// The page of a view. It is outside the component, so a render does not create a new component type for the same view.
function PageFor({ view }: { view: AppView }) {
	// The E2E build can force a render failure in one screen, so the per-page boundary is checked with a real window. A
	// production build removes the branch: Vite replaces `MODE` with `production`.
	if (e2eForcedFailure(view)) {
		throw new Error(`E2E forced render failure in ${view}`);
	}
	switch (view) {
		case "games":
			return <GamesPage />;
		case "mods":
			return <ModsPage />;
		case "settings":
			return <GameSettingsPage />;
		case "app":
			return <AppSettingsPage />;
		case "downloads":
			return <DownloadsPage />;
		case "notifications":
			return <NotificationsPage />;
		case "storage":
			return <StoragePage />;
		default:
			return assertNever(view);
	}
}

export function AppMain() {
	const { view } = useNavigation();

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col">
			<ErrorBoundary key={view}>
				<PageFor view={view} />
			</ErrorBoundary>
		</div>
	);
}

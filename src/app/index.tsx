import { AppProviders } from "@/app/app-providers";
import { AppShell } from "@/app/shell/app-shell";
import { CloseDialog } from "@/app/shell/close-dialog";
import { ErrorBoundary } from "@/components/app/error-boundary";

/// The app is a provider tree and a shell. Screens, dialogs and data hooks live in their own modules; this file is the
/// place where they meet, and it should stay short enough to read in one sitting.
function App() {
	return (
		<AppProviders>
			<ErrorBoundary>
				<AppShell />
			</ErrorBoundary>
			{/* Above the boundary: a window whose screens failed can still be closed. */}
			<CloseDialog />
		</AppProviders>
	);
}

export default App;

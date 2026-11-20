/// The shape of the E2E seam shared by the window and the WebDriver suite. Both sides import these types; the suite
/// installs the globals and the window reads them only in an `e2e` build.

/// Queued answers for the native file dialogs. An empty `files` array is a cancelled dialog.
export interface E2eDialogOverrides {
	files?: string[];
	file?: string | null;
}

/// The lifecycle events the workspace emits for backend work, forwarded by the e2e task seam.
export type E2eTaskEvent = "task-started" | "task-progress" | "task-finished" | "task-cancelled";

/// The backend task snapshot shape. The window derives the title from `kind` and the game name from `game`.
export interface E2eTaskPayload {
	id: string;
	game: string | null;
	kind: string;
	state: string;
	percent?: number | null;
}

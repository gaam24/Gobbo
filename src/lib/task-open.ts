/// A Working-panel Open action can belong to a game other than the one on screen. Switching games closes every shell
/// dialog, so the caller must select the task's game first and defer opening until that game is active. This pure
/// decision keeps the rule testable without a component, a context or a Tauri call.
export function needsGameSwitch(taskGameId: string, activeGameId: string | undefined): boolean {
	return activeGameId !== taskGameId;
}

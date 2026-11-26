/// The DOM questions every keyboard handler of the window asks before it acts, in one place: whether the user is typing
/// in a field, whether the key happened inside an overlay that owns its keys, and whether a modal dialog is open. The
/// global shortcuts, the mod list and the game editor read these, so a field or a dialog behaves the same everywhere.

/// The overlays whose keys are their own: dialogs, menus, and the list of an open select or combobox.
const OVERLAY = "[role='dialog'], [role='alertdialog'], [role='menu'], [role='listbox'], [role='combobox']";
const MODAL = "[role='dialog'], [role='alertdialog']";

/// The element an event came from, or `null` for the document or a non-element target.
export function elementOf(target: EventTarget | null): HTMLElement | null {
	return target instanceof HTMLElement ? target : null;
}

/// Whether the target is a text field, a text area or an editable element, whose keys belong to the typing.
export function isTypingTarget(target: EventTarget | null): boolean {
	return (
		target instanceof HTMLInputElement ||
		target instanceof HTMLTextAreaElement ||
		(target instanceof HTMLElement && target.isContentEditable)
	);
}

/// Whether an element is inside a dialog, a menu or a select, whose keys are their own.
export function inOverlay(element: HTMLElement | null): boolean {
	return Boolean(element?.closest(OVERLAY));
}

/// Whether a modal dialog is open anywhere in the window. A page-level shortcut waits until it closes.
export function hasOpenDialog(): boolean {
	return document.querySelector(MODAL) !== null;
}

/// Ctrl on Windows and Linux, Cmd on macOS: the modifier the app's shortcuts name "Ctrl".
export function hasCtrl(event: KeyboardEvent | MouseEvent): boolean {
	return event.ctrlKey || event.metaKey;
}

/// `MouseEvent.button` of the side buttons most mice have.
export const MOUSE_BACK = 3;
export const MOUSE_FORWARD = 4;

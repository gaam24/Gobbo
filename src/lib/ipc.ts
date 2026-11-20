import type { AppError, Message, Recovery } from "@/bindings";
import { hasTranslation, translateDynamic } from "@/i18n";

/// Result of a tauri-specta command: either the data, or the error from the backend.
export type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: AppError };

/// A failed command. It keeps the backend error, so the interface can show the translated text for its code.
export class CommandError extends Error {
	readonly appError: AppError;

	constructor(appError: AppError) {
		super(appError.code);
		this.name = "CommandError";
		this.appError = appError;
	}
}

export async function unwrap<T>(call: Promise<CommandResult<T>>): Promise<T> {
	const result = await call;

	if (result.status === "error") throw new CommandError(result.error);
	return result.data;
}

// Codes come from the backend at run time, so their keys are not in the typed key set. `hasTranslation` checks each
// code before the lookup, and a code without text shows the code itself.
/// The text for a code the backend sends. Plural forms read `count` as a number, and the backend sends every param as a
/// string.
export function translateCode(code: string, params: Record<string, string> = {}): string {
	const values: Record<string, string | number> = { ...params };

	if ("count" in params) values.count = Number(params.count);
	return translateDynamic(code, values);
}

/// The text for a message code, such as `file.no_conflict`. A code without a translation shows the code itself.
export function translate(message: Message): string {
	const key = `messages.${message.code}`;
	return hasTranslation(key) ? translateCode(key, message.params) : message.code;
}

/// The text for a backend error. Each error kind adds a short hint where one helps, such as "try again".
export function errorText(error: AppError): string {
	const key = `errors.${error.code}`;
	const text = hasTranslation(key) ? translateCode(key, error.params) : translateCode("errors.unexpected", { detail: error.code });
	const hint = error.kind === "retryable" || error.kind === "bug" ? translateCode(`errors.kinds.${error.kind}`, {}) : null;
	return hint ? `${text} ${hint}` : text;
}

/// The text to show for any failure: a backend error gets its translation, anything else its message.
export function messageOf(error: unknown): string {
	if (error instanceof CommandError) return errorText(error.appError);
	return error instanceof Error ? error.message : String(error);
}

/// The technical side of a failure for the notifications: the backend's error code, or the message of any other error.
export function detailOf(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

/// The backend's error code of a failed command, or null for any other error.
export function codeOf(error: unknown): string | null {
	return error instanceof CommandError ? error.appError.code : null;
}

/// Whether a command failed because the user stopped it, rather than because something went wrong. The backend marks
/// every stop code with a `cancelled` suffix (`import.cancelled`, `pack.link_cancelled`, `collection.cancelled`, ...),
/// so match the same rule instead of naming each feature's code.
export function cancelledOf(error: unknown): boolean {
	return error instanceof CommandError && error.appError.code.endsWith("cancelled");
}

/// Whether the backend found the exact archive already in the library. That is a neutral outcome, not a failed import:
/// the window points at the copy the user already has, and syncing the profile puts it on the list.
export function alreadyInLibraryOf(error: unknown): boolean {
	return error instanceof CommandError && error.appError.code === "install.already_in_library";
}

/// Whether a provider rejected a direct download because the account needs a paid tier.
export function paymentRequiredOf(error: unknown): boolean {
	return error instanceof CommandError && error.appError.code === "source.payment_required";
}

/// Whether a collection member needs the user's FOMOD choices before it can be imported.
export function needsChoiceOf(error: unknown): boolean {
	return error instanceof CommandError && error.appError.code === "collection.needs_choice";
}

/// The downloaded archive an update or a requirement could not install without the user's FOMOD choices, or null.
export function needsChoicePathOf(error: unknown): string | null {
	if (!(error instanceof CommandError) || error.appError.code !== "import.needs_choice") return null;
	return error.appError.params.path ?? null;
}

/// The concrete step the backend offers for a failure, or null when the message is enough.
export function recoveryOf(error: unknown): Recovery | null {
	return error instanceof CommandError ? (error.appError.recovery ?? null) : null;
}

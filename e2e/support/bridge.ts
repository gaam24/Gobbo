import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";

export interface CommandError {
	code: string;
	params: Record<string, string>;
	kind?: string;
	recovery?: unknown;
	message?: string;
}

export class CommandFailure extends Error {
	readonly code: string;
	readonly params: Record<string, string>;
	readonly kind: string | undefined;
	readonly recovery: unknown;

	constructor(error: CommandError) {
		super(`${error.code}${error.message ? `: ${error.message}` : ""}`);
		this.name = "CommandFailure";
		this.code = error.code;
		this.params = error.params;
		this.kind = error.kind;
		this.recovery = error.recovery;
	}
}

type Result<T> = { ok: true; value: T } | { ok: false; error: string };

/// Runs one Tauri command and returns its typed answer or the `AppError` the backend sent. The app's own bridge is used;
/// this is the same path the window takes, including the capability check and the Specta serialization. A failure is
/// carried as JSON text: WebDriver does not always serialize the backend's plain error object.
export async function invokeResult<T>(command: string, args: Record<string, unknown> = {}): Promise<Result<T>> {
	const raw = await browser.executeAsync(
		(cmd: string, callArgs: Record<string, unknown>, done: (result: unknown) => void) => {
			const failed = (error: unknown) => {
				const record = error && typeof error === "object" ? (error as Record<string, unknown>) : {};
				const payload = {
					code: typeof record.code === "string" ? record.code : "e2e.invoke_failed",
					params: record.params && typeof record.params === "object" ? record.params : {},
					kind: typeof record.kind === "string" ? record.kind : undefined,
					recovery: record.recovery,
					message: error instanceof Error ? error.message : String(error),
				};
				// The embedded driver treats a top-level `ok: false` as a failed executeAsync call. Keep the envelope
				// under a neutral key, so a rejected command is a value the spec can inspect.
				done({ e2e: { ok: false, error: JSON.stringify(payload) } });
			};
			void (async () => {
				try {
					const bridge = window.__TAURI__;

					if (!bridge) throw new Error("the Tauri bridge is not available");
					const value = await bridge.core.invoke(cmd, callArgs);
					done({ e2e: { ok: true, value } });
				} catch (error) {
					failed(error);
				}
			})();
		},
		command,
		args,
	);
	const envelope = raw as { e2e: { ok: true; value: T } | { ok: false; error: string } };
	const result = envelope.e2e;

	if (result.ok) return result;
	return { ok: false, error: result.error };
}

/// Invokes a command and throws `CommandFailure` for an `AppError`. Use it when a failure is a test failure.
export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
	const result = await invokeResult<T>(command, args);

	if (!result.ok) throw new CommandFailure(JSON.parse(result.error) as CommandError);
	return result.value;
}

/// Invokes a command that takes a progress `Channel`, and returns the answer together with every event the channel sent.
export async function invokeWithChannel<T>(command: string, args: Record<string, unknown> = {}): Promise<{ value: T; events: unknown[] }> {
	const raw = await browser.executeAsync(
		(cmd: string, callArgs: Record<string, unknown>, done: (result: unknown) => void) => {
			const events: unknown[] = [];
			const failed = (error: unknown) => {
				const record = error && typeof error === "object" ? (error as Record<string, unknown>) : {};
				const payload = {
					code: typeof record.code === "string" ? record.code : "e2e.invoke_failed",
					params: record.params && typeof record.params === "object" ? record.params : {},
					kind: typeof record.kind === "string" ? record.kind : undefined,
					recovery: record.recovery,
					message: error instanceof Error ? error.message : String(error),
				};
				done({ e2e: { ok: false, error: JSON.stringify(payload), events } });
			};
			void (async () => {
				try {
					const bridge = window.__TAURI__;

					if (!bridge) throw new Error("the Tauri bridge is not available");
					const progress = new bridge.core.Channel();
					progress.onmessage = (message: unknown) => events.push(message);
					const value = await bridge.core.invoke(cmd, { ...callArgs, progress });
					done({ e2e: { ok: true, value, events } });
				} catch (error) {
					failed(error);
				}
			})();
		},
		command,
		args,
	);
	const envelope = raw as {
		e2e: { ok: true; value: T; events: unknown[] } | { ok: false; error: string; events: unknown[] };
	};
	const result = envelope.e2e;

	if (!result.ok) throw new CommandFailure(JSON.parse(result.error) as CommandError);
	return { value: result.value, events: result.events };
}

/// A command expected to fail. The caller asserts the stable `AppError` code.
export async function invokeError(command: string, args: Record<string, unknown> = {}): Promise<CommandError> {
	const result = await invokeResult<unknown>(command, args);
	assert.equal(result.ok, false, `${command} was expected to fail`);
	const failure = result as { ok: false; error: string };
	return JSON.parse(failure.error) as CommandError;
}

/// Reloads the webview, as the user's Reload would. The Tauri session and the backend state stay alive.
export async function reloadWindow(): Promise<void> {
	try {
		await browser.execute(() => {
			window.location.reload();
		});
	} catch {
		// A navigation can close the execution context before the answer arrives.
	}

	await waitForShell();
}

export async function waitForShell(timeout = 60_000): Promise<void> {
	await browser.waitUntil(
		async () => {
			const title = await browser.getTitle();
			const rendered = await browser.execute(() => document.querySelector("#root")?.childElementCount ?? 0);
			return title.includes("Gobbo") && rendered > 0;
		},
		{ timeout, timeoutMsg: "the Gobbo window did not render its shell" },
	);
}

import { expect, test } from "bun:test";

import type { AppError } from "@/bindings";
import { applyLanguage } from "@/i18n";
import {
	alreadyInLibraryOf,
	CommandError,
	cancelledOf,
	codeOf,
	detailOf,
	errorText,
	messageOf,
	needsChoiceOf,
	paymentRequiredOf,
	recoveryOf,
	translate,
	translateCode,
	unwrap,
} from "@/lib/ipc";

await applyLanguage("en");

function appError(code: string, kind: AppError["kind"] = "permanent", recovery?: AppError["recovery"]): AppError {
	return { code, params: {}, kind, recovery };
}

test("unwrap returns data and turns a backend error into a CommandError", async () => {
	expect(await unwrap(Promise.resolve({ status: "ok", data: 42 }))).toBe(42);

	try {
		await unwrap(Promise.resolve({ status: "error", error: appError("state.busy", "retryable") }));
		throw new Error("the error was not thrown");
	} catch (error) {
		expect(error).toBeInstanceOf(CommandError);
		expect((error as CommandError).appError.code).toBe("state.busy");
	}
});

test("translate uses a known message code and falls back to the code", () => {
	expect(translate({ code: "file.no_conflict", params: {} })).toBe("No conflict");
	expect(translate({ code: "file.unknown_code", params: {} })).toBe("file.unknown_code");
});

test("translateCode reads a status code and pluralizes its string count", () => {
	expect(translateCode("health.overwrite")).toBe("The game or a tool wrote files while the mods were applied.");
	expect(translateCode("health.requirements", { count: "1" })).toBe("1 mod needs something that's missing.");
	expect(translateCode("health.requirements", { count: "8" })).toBe("8 mods need something that's missing.");
	expect(translateCode("health.unknown_code")).toBe("health.unknown_code");
});

test("errorText translates a known error and adds the retry hint", () => {
	const text = errorText(appError("state.busy", "retryable"));
	expect(text).toContain("Gobbo is still busy");
	expect(text).toContain("Try again");
});

test("errorText falls back to the unexpected text with the code as detail", () => {
	expect(errorText(appError("no.such_code"))).toContain("no.such_code");
});

test("messageOf, detailOf and cancelledOf use the backend error", () => {
	const error = new CommandError(appError("import.cancelled"));
	expect(messageOf(error)).toContain("import was stopped");
	expect(detailOf(error)).toBe("import.cancelled");
	expect(cancelledOf(error)).toBe(true);
	expect(cancelledOf(new Error("other"))).toBe(false);
	expect(messageOf("plain")).toBe("plain");
	expect(detailOf("plain")).toBe("plain");
});

test("cancelledOf matches a stop code by its cancelled suffix", () => {
	// Two unrelated families prove the suffix rule; naming every backend code here would duplicate the rule it tests.
	expect(cancelledOf(new CommandError(appError("import.cancelled")))).toBe(true);
	expect(cancelledOf(new CommandError(appError("collection.cancelled")))).toBe(true);
	expect(cancelledOf(new CommandError(appError("download.cancelling")))).toBe(false);
	expect(cancelledOf(new CommandError(appError("cancelled.download")))).toBe(false);
});

test("alreadyInLibraryOf identifies the duplicate-archive outcome and ignores other failures", () => {
	expect(alreadyInLibraryOf(new CommandError(appError("install.already_in_library")))).toBe(true);
	expect(alreadyInLibraryOf(new CommandError(appError("import.bad_archive")))).toBe(false);
	expect(alreadyInLibraryOf(new Error("install.already_in_library"))).toBe(false);
});

test("codeOf reads the code of a backend failure and ignores other errors", () => {
	// A generic fake error stands in for any command failure; no game or mod data is involved.
	const failure = new CommandError(appError("state.busy"));
	expect(codeOf(failure)).toBe("state.busy");
	expect(codeOf(new Error("plain"))).toBeNull();
	expect(codeOf("plain")).toBeNull();
});

test("needsChoiceOf reads the code, not the message of a failure", () => {
	expect(needsChoiceOf(new CommandError(appError("collection.needs_choice")))).toBe(true);
	expect(needsChoiceOf(new CommandError(appError("other")))).toBe(false);
	expect(needsChoiceOf(new Error("collection.needs_choice"))).toBe(false);
});

test("paymentRequiredOf and recoveryOf read the typed error", () => {
	const paid = new CommandError(appError("source.payment_required"));
	expect(paymentRequiredOf(paid)).toBe(true);
	expect(paymentRequiredOf(new CommandError(appError("other")))).toBe(false);

	const recovery = { action: "open_log" } as const;
	expect(recoveryOf(new CommandError(appError("x", "permanent", recovery)))).toEqual(recovery);
	expect(recoveryOf(new Error("x"))).toBeNull();
});

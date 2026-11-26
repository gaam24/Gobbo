/// <reference types="bun" />
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import en from "./en";
import pl from "./pl";

interface Messages {
	[key: string]: string | Messages;
}

function flatten(messages: Messages, prefix = ""): Map<string, string> {
	const out = new Map<string, string>();

	for (const [key, value] of Object.entries(messages)) {
		const path = prefix + key;

		if (typeof value === "string") out.set(path, value);
		else for (const [inner, text] of flatten(value, `${path}.`)) out.set(inner, text);
	}

	return out;
}

/// Separate test files hold fixtures and assertions; their codes are sentinels, never messages sent to the window.
const TEST_ONLY_NAMES = new Set(["tests.rs", "test_support.rs", "bench_api.rs"]);
const PATH_SEPARATOR = /[\\/]/;

function isTestOnlyFile(path: string): boolean {
	const parts = path.split(PATH_SEPARATOR);
	return TEST_ONLY_NAMES.has(parts[parts.length - 1] ?? "") || parts.slice(0, -1).includes("tests");
}

function rustFiles(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);

		if (entry.isDirectory()) return rustFiles(path);
		return path.endsWith(".rs") && !isTestOnlyFile(path) ? [path] : [];
	});
}

/// Only dotted codes are stable IPC codes; a bare literal is a filename or a test fixture id.
const CODE_PATTERN = /^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/;
const ERROR_CALL_PATTERN = /\bAppError::(?:new|user|retryable)\("([^"]+)"/g;
const CODE_CONST_PATTERN = /^pub const ([A-Z_]+): &str = "([^"]+)";/gm;

/// A dotted string is not necessarily an IPC error code: paths, ids and event names share the shape. Such constants
/// are named after what they hold, so their suffix tells them apart from a code without weakening the guard for a new
/// constant whose name does not follow one of these patterns.
const NON_CODE_CONST = /(?:_FILE|_DIR|_SUFFIX|_NAME|_ID|_EVENT|_MARKER|_VARIABLE|_TEMPLATE|_ENV)$/;
/// A status message key, translated from `messages` rather than `errors`, is not a code the window receives from an
/// `AppError`.
const MESSAGE_CODES = new Set(["status.loader_missing"]);

function isCode(text: string): boolean {
	return CODE_PATTERN.test(text);
}

/// Every error code the backend creates from a literal. A code built from a variable or named as a constant is checked
/// where it is used. The pattern covers all three constructors, so a new user-action or retryable error cannot drift
/// unnoticed — the old guard checked only `AppError::new`.
function errorCodes(dir: string): Set<string> {
	const codes = new Set<string>();

	for (const file of rustFiles(dir)) {
		for (const line of readFileSync(file, "utf8").split("\n")) {
			// Test-only codes are not sent to the window. Inline test modules sit at the end of a file.
			if (line.includes("#[cfg(test)]")) break;
			// A code used only inside an assertion is a test sentinel, not one the app can send.
			if (line.includes("assert")) continue;

			for (const match of line.matchAll(ERROR_CALL_PATTERN)) {
				const code = match[1];

				if (code !== undefined && isCode(code)) codes.add(code);
			}
		}
	}

	return codes;
}

/// A canonical code passed as a named constant is not a literal at the emission site, so read it from its definition.
/// Every production file is scanned: provider codes live beside their adapters, not in `error.rs`, and would otherwise
/// drift without a message. Test-only files are skipped by `rustFiles` just as they are for literal codes.
function namedCodes(dir: string): Set<string> {
	const codes = new Set<string>();

	for (const file of rustFiles(dir)) {
		for (const match of readFileSync(file, "utf8").matchAll(CODE_CONST_PATTERN)) {
			const name = match[1];
			const code = match[2];

			if (name === undefined || NON_CODE_CONST.test(name)) continue;
			if (code === undefined || !isCode(code) || MESSAGE_CODES.has(code)) continue;

			codes.add(code);
		}
	}

	return codes;
}

const english = flatten(en.errors);
const polish = flatten(pl.errors);
const sourceDir = join(import.meta.dir, "../../src-tauri/src");
const codes = new Set([...errorCodes(sourceDir), ...namedCodes(sourceDir)]);

describe("test-only Rust files", () => {
	test("separate test files and files under tests/ are skipped", () => {
		expect(isTestOnlyFile(join("src", "module", "tests.rs"))).toBe(true);
		expect(isTestOnlyFile(join("src", "test_support.rs"))).toBe(true);
		expect(isTestOnlyFile(join("src", "bench_api.rs"))).toBe(true);
		expect(isTestOnlyFile(join("src", "module", "tests", "helper.rs"))).toBe(true);
	});

	test("production files are still scanned", () => {
		expect(isTestOnlyFile(join("src", "module", "commands.rs"))).toBe(false);
		expect(isTestOnlyFile(join("src", "error.rs"))).toBe(false);
	});
});

describe("backend error codes", () => {
	test("named codes are read from every production module", () => {
		expect(codes.has("nexus.key_rejected")).toBe(true);
		expect(codes.has("thunderstore.not_found")).toBe(true);
		// File names and status message keys share the dotted shape but are not error codes.
		expect(codes.has("config.json")).toBe(false);
		expect(codes.has("status.loader_missing")).toBe(false);
	});

	test("every literal backend error code has an English message", () => {
		expect([...codes].filter((code) => !english.has(code)).toSorted()).toEqual([]);
	});

	test("every literal backend error code has a Polish message", () => {
		expect([...codes].filter((code) => !polish.has(code)).toSorted()).toEqual([]);
	});
});

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

function rustFiles(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);
		if (entry.isDirectory()) return rustFiles(path);
		return path.endsWith(".rs") ? [path] : [];
	});
}

/// Only dotted codes are stable IPC codes; a bare literal is a filename or a test fixture id.
const CODE_PATTERN = /^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/;
const ERROR_CALL_PATTERN = /\bAppError::(?:new|user|retryable)\("([^"]+)"/g;
const CODE_CONST_PATTERN = /^pub const [A-Z_]+: &str = "([^"]+)";/gm;

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
/// Without this, a code used only through a constant would silently escape the guard.
function namedCodes(errorFile: string): Set<string> {
	const codes = new Set<string>();
	for (const match of readFileSync(errorFile, "utf8").matchAll(CODE_CONST_PATTERN)) {
		const code = match[1];
		if (code !== undefined && isCode(code)) codes.add(code);
	}
	return codes;
}

const english = flatten(en.errors);
const polish = flatten(pl.errors);
const sourceDir = join(import.meta.dir, "../../src-tauri/src");
const codes = new Set([...errorCodes(sourceDir), ...namedCodes(join(sourceDir, "error.rs"))]);

describe("backend error codes", () => {
	test("every literal backend error code has an English message", () => {
		expect([...codes].filter((code) => !english.has(code)).toSorted()).toEqual([]);
	});

	test("every literal backend error code has a Polish message", () => {
		expect([...codes].filter((code) => !polish.has(code)).toSorted()).toEqual([]);
	});
});

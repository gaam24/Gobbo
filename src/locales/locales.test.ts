/// <reference types="bun" />
import { describe, expect, test } from "bun:test";

import en from "./en";
import { resolveLocale } from "./locale";
import pl from "./pl";

interface Messages {
	[key: string]: string | Messages;
}

const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/;
const PLACEHOLDER = /{{\s*(\w+)\s*}}/g;

/// Flattens nested messages to dotted keys, e.g. `settings.language.system`.
function flatten(messages: Messages, prefix = ""): Map<string, string> {
	const out = new Map<string, string>();
	for (const [key, value] of Object.entries(messages)) {
		const path = prefix + key;
		if (typeof value === "string") out.set(path, value);
		else for (const [inner, text] of flatten(value, `${path}.`)) out.set(inner, text);
	}
	return out;
}

/// The key without its plural suffix, so `changes_one` and `changes_few` share the base `changes`.
const baseOf = (key: string) => key.replace(PLURAL_SUFFIX, "");

function placeholders(text: string): string[] {
	return [...text.matchAll(PLACEHOLDER)].map((match) => match[1] ?? "").toSorted();
}

const englishKeys = flatten(en);
const polishKeys = flatten(pl);
const englishBases = new Set([...englishKeys.keys()].map((key) => baseOf(key)));
const polishBases = new Set([...polishKeys.keys()].map((key) => baseOf(key)));
const polishCategories = new Intl.PluralRules("pl").resolvedOptions().pluralCategories;

describe("translations", () => {
	test("every English key has a Polish translation", () => {
		const missing: string[] = [];
		for (const key of englishKeys.keys()) {
			const base = baseOf(key);
			const isPlural = base !== key;
			if (!isPlural) {
				if (!polishKeys.has(key)) missing.push(key);
				continue;
			}
			for (const category of polishCategories) {
				if (!polishKeys.has(`${base}_${category}`)) missing.push(`${base}_${category}`);
			}
		}
		expect(missing).toEqual([]);
	});

	test("placeholders match the English text", () => {
		const mismatched: string[] = [];
		for (const [key, text] of polishKeys) {
			const base = baseOf(key);
			const source = englishKeys.get(key) ?? englishKeys.get(`${base}_other`) ?? englishKeys.get(`${base}_one`);
			if (source === undefined) continue;
			if (JSON.stringify(placeholders(text)) !== JSON.stringify(placeholders(source))) mismatched.push(key);
		}
		expect(mismatched).toEqual([]);
	});

	test("every Polish plural base is complete", () => {
		expect(polishBases).toEqual(englishBases);
	});
});

describe("resolveLocale", () => {
	test("a fixed setting wins over the browser languages", () => {
		expect(resolveLocale("pl", ["en-US"])).toBe("pl");
		expect(resolveLocale("en", ["pl-PL"])).toBe("en");
	});

	test("system picks the first language that has a translation", () => {
		expect(resolveLocale("system", ["de-DE", "pl-PL"])).toBe("pl");
		expect(resolveLocale("system", ["pl-PL", "en-US"])).toBe("pl");
		expect(resolveLocale("system", ["EN-us"])).toBe("en");
	});

	test("system falls back to English when no language has a translation", () => {
		expect(resolveLocale("system", ["de-DE", "fr"])).toBe("en");
		expect(resolveLocale("system", [])).toBe("en");
	});
});

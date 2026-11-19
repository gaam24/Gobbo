import { describe, expect, test } from "bun:test";
import {
	EMPTY_DRAFT,
	inferLayout,
	localIssues,
	normalizeDraft,
	parseDraft,
	serializeDraft,
	setLoader,
	slugify,
} from "@/views/games/editor/draft";

describe("the game definition draft", () => {
	test("a title becomes a stable slug", () => {
		expect(slugify("The Witcher 3: Wild Hunt")).toBe("the-witcher-3-wild-hunt");
		expect(slugify("Zażółć gęślą jaźń")).toBe("zazolc-gesla-jazn");
		expect(slugify("...")).toBe("game");
	});

	test("a new draft has no optional tables", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "example",
			name: "Example Game",
			executables: [{ name: "Example", path: "Example.exe" }],
			deploy: { slots: [{ dir: "Mods", flat: false, prefix: "" }] },
		});
		expect(text).toContain('id = "example"');
		expect(text).toContain("[[executables]]");
		expect(text).toContain("[[deploy.slots]]");
		expect(text).not.toContain("flat = false");
		expect(text).not.toContain('prefix = ""');
	});

	test("a loader string and a named variant round-trip", () => {
		const base = { ...EMPTY_DRAFT, id: "unity", name: "Unity" };
		expect(serializeDraft(setLoader(base, "bepinex", ""))).toContain('loader = "bepinex"');
		const variant = setLoader(base, "bepinex", "6");
		const text = serializeDraft(variant);
		expect(text).toContain("[loader]");
		expect(text).toContain('variant = "6"');
		const parsed = parseDraft(text);
		expect(parsed.loader).toEqual({ id: "bepinex", variant: "6" });
	});

	test("the defaults a game does not need are left out", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "game",
			name: "Game",
			user_data: [
				{
					id: "saves",
					path: "{documents}/Game",
					kind: "user-data",
					mode: "redirect",
					default: "shared",
					inherit: true,
				},
			],
			load_order: { format: "mods_settings", file: "{documents}/Game/mods.settings", priority_min: 1, priority_max: 9999 },
		});
		expect(text).not.toContain("kind =");
		expect(text).not.toContain("mode =");
		expect(text).not.toContain("default =");
		expect(text).not.toContain("inherit =");
		expect(text).not.toContain("priority_min");
	});

	test("a missing detect and deploy section opens in the form", () => {
		const draft = normalizeDraft({ schema: 1, id: "x", name: "X" });
		expect(draft.detect).toEqual({});
		expect(draft.deploy).toEqual({});
		expect(draft.executables).toEqual([]);
	});

	test("a full-root draft writes the root and the move rules", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "game",
			name: "Game",
			executables: [{ name: "Game", path: "Game.exe" }],
			deploy: { full_root: true, move: [{ extensions: [".archive", ".xl"], to: "archive/pc/mod" }] },
		});
		expect(text).toContain("full_root = true");
		expect(text).toContain("[[deploy.move]]");
		expect(text).toContain('to = "archive/pc/mod"');
		const parsed = parseDraft(text);
		expect(parsed.deploy.full_root).toBe(true);
		expect(parsed.deploy.move).toEqual([{ extensions: [".archive", ".xl"], to: "archive/pc/mod" }]);
		expect(
			localIssues({
				...EMPTY_DRAFT,
				id: "game",
				name: "Game",
				executables: [{ name: "Game", path: "Game.exe" }],
				deploy: { full_root: true },
			}),
		).toEqual([]);
	});

	test("the form names the fields that block a save", () => {
		expect(localIssues(EMPTY_DRAFT)).toEqual(["name", "id", "executable", "layout"]);
		const draft = {
			...EMPTY_DRAFT,
			id: "example",
			name: "Example",
			executables: [{ name: "Example", path: "Example.exe" }],
			deploy: { slots: [{ dir: "Mods" }] },
		};
		expect(localIssues(draft)).toEqual([]);
	});

	test("log sources keep their path and omit the defaults", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "game",
			name: "Game",
			logs: [
				{ kind: "script", path: "r6/logs", pattern: "*CURRENT.log", limit: 1 },
				{ kind: "crash", path: ".", pattern: "*.txt", limit: 1, recursive: false },
			],
		});
		expect(text).toContain("[[logs]]");
		expect(text).toContain('kind = "script"');
		expect(text).toContain('path = "r6/logs"');
		expect(text).toContain('pattern = "*CURRENT.log"');
		expect(text).toContain("recursive = false");
		expect(parseDraft(text).logs).toHaveLength(2);
	});

	test("an existing definition picks the layout card it belongs to", () => {
		expect(inferLayout({ ...EMPTY_DRAFT, loader: "bepinex" })).toBe("loader");
		expect(inferLayout({ ...EMPTY_DRAFT, deploy: { slots: [{ dir: "Mods" }] } })).toBe("slot");
		expect(inferLayout({ ...EMPTY_DRAFT, deploy: { mirror: [{ dir: "Data" }] } })).toBe("mirror");
		expect(inferLayout({ ...EMPTY_DRAFT, deploy: { slots: [{ dir: "Mods" }], mirror: [{ dir: "Data" }] } })).toBe("custom");
	});
});

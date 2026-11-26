import { describe, expect, test } from "bun:test";
import {
	EMPTY_DRAFT,
	idForName,
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
		expect(slugify("Example Game 3: Wild Hunt")).toBe("example-game-3-wild-hunt");
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

	test("an app version floor survives a round trip through the editor", () => {
		const text = 'schema = 1\nid = "example"\nname = "Example"\nmin_gobbo = "0.3.0"\n[detect]\n[deploy]\n';
		expect(serializeDraft(parseDraft(text))).toContain('min_gobbo = "0.3.0"');
	});

	test("a native Linux marker survives a round trip through the editor", () => {
		const text =
			'schema = 1\nid = "native"\nname = "Native"\n[detect]\nmarkers = ["Game.exe"]\nlinux_markers = ["Game.x86_64"]\n[deploy]\n';
		const draft = parseDraft(text);
		expect(draft.detect.linux_markers).toEqual(["Game.x86_64"]);
		const written = serializeDraft(draft);
		expect(written).toContain('linux_markers = [ "Game.x86_64" ]');
		expect(parseDraft(written).detect.linux_markers).toEqual(["Game.x86_64"]);
	});

	test("a loader package survives a round trip through the editor", () => {
		const text =
			'schema = 1\nid = "unreal"\nname = "Unreal"\n[install]\nname = "Example Loader"\nversion = "4.0.0"\nto = "Binaries/Win64"\ncheck = ["Binaries/Win64/example-loader/example-loader.dll"]\nurl = "https://example.com/example-loader.zip"\nsha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"\n[detect]\n[deploy]\n[[deploy.slots]]\ndir = "Mods"\n';
		const written = serializeDraft(parseDraft(text));
		expect(written).toContain("[install]");
		expect(written).toContain('name = "Example Loader"');
		expect(written).toContain('version = "4.0.0"');
		expect(written).toContain('to = "Binaries/Win64"');
	});

	test("the loader-only slot fields survive a round trip through the editor", () => {
		const text =
			'schema = 1\nid = "unreal"\nname = "Unreal"\n[deploy]\n[[deploy.slots]]\ndir = "Binaries/Win64/ue4ss/Mods"\nmarkers = ["Scripts", "dlls"]\nenable_file = "enabled.txt"\n';
		const draft = parseDraft(text);
		expect(draft.deploy.slots?.[0]?.markers).toEqual(["Scripts", "dlls"]);
		expect(draft.deploy.slots?.[0]?.enable_file).toBe("enabled.txt");
		const written = serializeDraft(draft);
		expect(written).toContain('markers = [ "Scripts", "dlls" ]');
		expect(written).toContain('enable_file = "enabled.txt"');
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
			load_order: {
				format: "mods_settings",
				file: "{documents}/Game/mods.settings",
				priority_min: 1,
				priority_max: 9999,
			},
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
			deploy: {
				full_root: true,
				move: [{ extensions: [".archive", ".xl"], to: "archive/pc/mod" }],
			},
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
		// An empty name hides the id line: the id is made from the name.
		expect(localIssues(EMPTY_DRAFT)).toEqual(["name", "executable", "layout"]);
		expect(localIssues({ ...EMPTY_DRAFT, name: "Game", id: "Not An Id" })).toContain("id");
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
				{
					kind: "crash",
					path: ".",
					pattern: "*.txt",
					limit: 1,
					recursive: false,
				},
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
		expect(
			inferLayout({
				...EMPTY_DRAFT,
				deploy: { slots: [{ dir: "Mods" }], mirror: [{ dir: "Data" }] },
			}),
		).toBe("custom");
	});
});

describe("the id follows the name until the user types one", () => {
	test("typing a name fills in its whole slug, not only the first letter", () => {
		let draft = { ...EMPTY_DRAFT, name: "", id: "" };

		for (const name of ["E", "Ex", "Example", "Example G", "Example Game"]) {
			draft = { ...draft, name, id: idForName(draft, name) };
		}

		expect(draft.id).toBe("example-game");
	});

	test("an id the user typed is kept when the name changes", () => {
		const draft = { ...EMPTY_DRAFT, name: "Example", id: "custom-id" };
		expect(idForName(draft, "Example Two")).toBe("custom-id");
	});

	test("a blank name suggests no id, and an id that was the name's slug follows a new name", () => {
		expect(idForName({ ...EMPTY_DRAFT, name: "Example", id: "example" }, "")).toBe("");
		expect(idForName({ ...EMPTY_DRAFT, name: "Example", id: "example" }, "Example Two")).toBe("example-two");
	});
});

describe("the editor writes the rules' own spelling", () => {
	test("an extension typed in capitals is written in lower case", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "example",
			name: "Example",
			executables: [{ name: "Example", path: "Example.exe" }],
			deploy: {
				slots: [{ dir: "Mods", root_extensions: [".PAK", " .Zip "] }],
				mirror: [{ dir: "Data", extensions: [".ESP"] }],
			},
		});
		const parsed = parseDraft(text);
		expect(parsed.deploy.slots?.[0]?.root_extensions).toEqual([".pak", ".zip"]);
		expect(parsed.deploy.mirror?.[0]?.extensions).toEqual([".esp"]);
	});

	test("a move target typed with a backslash or a final slash is written as a folder", () => {
		const text = serializeDraft({
			...EMPTY_DRAFT,
			id: "example",
			name: "Example",
			executables: [{ name: "Example", path: "Example.exe" }],
			deploy: {
				move: [{ to: `${["archive", "pc", "mod"].join("\\")}/`, extensions: [".archive"] }],
			},
		});
		expect(parseDraft(text).deploy.move?.[0]?.to).toBe("archive/pc/mod");
	});
});

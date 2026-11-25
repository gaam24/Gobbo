import { expect, test } from "bun:test";

import type { EngineSuggestion_Serialize, FolderInspection_Serialize } from "@/bindings";
import { EMPTY_DRAFT, type GameDraft, loaderId } from "@/views/games/editor/draft";
import { applySuggestions } from "@/views/games/editor/suggestions";

/// A folder inspection with nothing found, so each test sets only the signal it checks.
function inspection(overrides: Partial<FolderInspection_Serialize>): FolderInspection_Serialize {
	return {
		folder: "/games/example",
		name: "Example Game",
		suggested_id: "example-game",
		executables: [],
		directories: [],
		markers: [],
		steam: null,
		loaders: [],
		engine: null,
		stores: { gog: [], epic: [], lutris: [] },
		...overrides,
	};
}

const unity: EngineSuggestion_Serialize = {
	loader_id: "bepinex",
	loader_name: "BepInEx",
	engine_name: "Unity",
	variant: "6",
	label: "IL2CPP",
	root: null,
};

const unreal: EngineSuggestion_Serialize = {
	loader_id: "unreal",
	loader_name: "Unreal Engine",
	engine_name: "Unreal Engine",
	variant: "ue4ss-4",
	label: null,
	root: "Project",
};

test("an engine with no installed loader selects its loader and variant", () => {
	const next = applySuggestions(EMPTY_DRAFT, inspection({ engine: unity }));
	expect(next.loader).toEqual({ id: "bepinex", variant: "6" });
	expect(next.deploy.root).toBeUndefined();
});

test("the project folder of an engine becomes the deployment root of its loader", () => {
	const next = applySuggestions(EMPTY_DRAFT, inspection({ engine: unreal }));
	expect(loaderId(next)).toBe("unreal");
	expect(next.loader).toEqual({ id: "unreal", variant: "ue4ss-4" });
	expect(next.deploy.root).toBe("{game}/Project");
});

test("a loader that is already installed is preferred to the engine's", () => {
	const next = applySuggestions(EMPTY_DRAFT, inspection({ engine: unity, loaders: [{ id: "bepinex", name: "BepInEx", variant: "5" }] }));
	expect(next.loader).toEqual({ id: "bepinex", variant: "5" });
});

test("a project folder the user already set is kept", () => {
	const draft: GameDraft = { ...EMPTY_DRAFT, deploy: { root: "{game}/Mine" } };
	const next = applySuggestions(draft, inspection({ engine: unreal }));
	expect(next.deploy.root).toBe("{game}/Mine");
});

test("a draft with its own mod layout gets no loader from the engine", () => {
	const draft: GameDraft = { ...EMPTY_DRAFT, deploy: { slots: [{ dir: "Mods" }] } };
	const next = applySuggestions(draft, inspection({ engine: unreal }));
	expect(next.loader).toBeUndefined();
	expect(next.deploy.root).toBeUndefined();
	expect(next.deploy.slots).toEqual([{ dir: "Mods" }]);
});

test("the project folder is set only for the engine's own loader", () => {
	const draft: GameDraft = { ...EMPTY_DRAFT, loader: "bepinex" };
	const next = applySuggestions(draft, inspection({ engine: unreal }));
	expect(next.loader).toBe("bepinex");
	expect(next.deploy.root).toBeUndefined();
});

test("applying the same suggestion twice changes nothing", () => {
	const once = applySuggestions(EMPTY_DRAFT, inspection({ engine: unreal }));
	expect(applySuggestions(once, inspection({ engine: unreal }))).toEqual(once);
});

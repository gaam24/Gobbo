import { expect, test } from "bun:test";
import type { FomodChoice } from "@/lib/fomod";
import { fomodWizardKey, forgetChoices, rememberChoices, storedChoices } from "@/lib/use-import";

const FIRST_WIZARD = fomodWizardKey("archive.zip", "Installer");
const OTHER_WIZARD = fomodWizardKey("other.zip", "Installer");

function choices(option: string): FomodChoice[] {
	return [{ step: 0, group_index: 0, group: "Options", option }];
}

test("saved choices are found by game and wizard key", () => {
	const cache = new Map<string, FomodChoice[]>();
	const answers = choices("First");
	rememberChoices(cache, "game-one", FIRST_WIZARD, answers);
	expect(storedChoices(cache, "game-one", FIRST_WIZARD)).toEqual(answers);
});

test("a wizard without saved choices has no answers", () => {
	const cache = new Map<string, FomodChoice[]>();
	expect(storedChoices(cache, "game-one", FIRST_WIZARD)).toBeUndefined();
});

test("a different game never reads another game's answers", () => {
	const cache = new Map<string, FomodChoice[]>();
	rememberChoices(cache, "game-one", FIRST_WIZARD, choices("First"));
	expect(storedChoices(cache, "game-two", FIRST_WIZARD)).toBeUndefined();
});

test("a different wizard of the same game never reads the first wizard's answers", () => {
	const cache = new Map<string, FomodChoice[]>();
	rememberChoices(cache, "game-one", FIRST_WIZARD, choices("First"));
	expect(storedChoices(cache, "game-one", OTHER_WIZARD)).toBeUndefined();
});

test("forgetting one wizard keeps the other answers of the same game", () => {
	const cache = new Map<string, FomodChoice[]>();
	rememberChoices(cache, "game-one", FIRST_WIZARD, choices("First"));
	rememberChoices(cache, "game-one", OTHER_WIZARD, choices("Second"));
	forgetChoices(cache, "game-one", FIRST_WIZARD);
	expect(storedChoices(cache, "game-one", FIRST_WIZARD)).toBeUndefined();
	expect(storedChoices(cache, "game-one", OTHER_WIZARD)).toEqual(choices("Second"));
});

test("stored answers are copies on the way in and on the way out", () => {
	const cache = new Map<string, FomodChoice[]>();
	const answers = choices("First");
	rememberChoices(cache, "game-one", FIRST_WIZARD, answers);
	answers.push({ step: 1, group_index: 0, group: "More", option: "Extra" });
	expect(storedChoices(cache, "game-one", FIRST_WIZARD)).toEqual(choices("First"));
	const restored = storedChoices(cache, "game-one", FIRST_WIZARD);
	if (restored) restored.push({ step: 2, group_index: 0, group: "Last", option: "Later" });
	expect(storedChoices(cache, "game-one", FIRST_WIZARD)).toEqual(choices("First"));
});

test("the wizard key keeps the file name and the module name apart", () => {
	const first = fomodWizardKey("archive.zip", "Installer");
	expect(first).toBe(fomodWizardKey("archive.zip", "Installer"));
	expect(first).not.toBe(fomodWizardKey("archive.zip", "Other"));
	expect(first).not.toBe(fomodWizardKey("other.zip", "Installer"));
});

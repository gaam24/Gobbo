import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { FolderInspection_Serialize } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { EMPTY_DRAFT, localIssues } from "@/views/games/editor/draft";
import { QuickSetup } from "@/views/games/editor/quick-setup";

await applyLanguage("en");

const draft = { ...EMPTY_DRAFT, id: "example", name: "Example Game" };

function inspectionWith(stores: FolderInspection_Serialize["stores"]): FolderInspection_Serialize {
	return {
		folder: "/games/example",
		name: "Example Game",
		suggested_id: "example-game",
		executables: [],
		directories: [],
		markers: [],
		steam: null,
		loaders: [],
		stores,
	};
}

function render(idWarning: string | null, inspection: FolderInspection_Serialize | null = null) {
	return renderToString(
		<QuickSetup
			draft={draft}
			onChange={() => {
				// The form is presentational in this check.
			}}
			folder=""
			onFolderChange={() => {
				// The folder is not part of this check.
			}}
			onBrowse={() => {
				// The dialog is not part of this check.
			}}
			inspection={inspection}
			inspecting={false}
			inspectError={null}
			presets={[]}
			layout="slot"
			onLayoutChange={() => {
				// The layout is not part of this check.
			}}
			check={null}
			issues={localIssues(draft)}
			editing={false}
			idWarning={idWarning}
		/>,
	);
}

test("the quick step names the folder, the layout and the missing program", () => {
	const html = render(null);
	expect(html).toContain("The game");
	expect(html).toContain("Game folder");
	expect(html).toContain("How mods get in");
	expect(html).toContain("A mods folder");
	expect(html).toContain("The game needs a program to start.");
});

test("an id that belongs to another definition is shown next to the field", () => {
	const html = render("A user definition with this id already exists. Choose another or edit that game.");
	expect(html).toContain("A user definition with this id already exists");
});

test("the quick step shows a badge for each store the folder was recognized by", () => {
	const html = render(
		null,
		inspectionWith({
			gog: ["1495134320"],
			epic: ["ExampleGame"],
			lutris: ["example-lutris"],
		}),
	);
	expect(html).toContain("GOG");
	expect(html).toContain("1495134320");
	expect(html).toContain("Epic");
	expect(html).toContain("ExampleGame");
	expect(html).toContain("Lutris");
	expect(html).toContain("example-lutris");
});

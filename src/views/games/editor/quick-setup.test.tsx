import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { EMPTY_DRAFT, localIssues } from "@/views/games/editor/draft";
import { QuickSetup } from "@/views/games/editor/quick-setup";

await applyLanguage("en");

const draft = { ...EMPTY_DRAFT, id: "example", name: "Example Game" };

function render(idWarning: string | null) {
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
			inspection={null}
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

import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { GameView } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { GameRow } from "@/views/games/game-row";

await applyLanguage("en");

const nothing = () => {
	// The row actions are not part of this check.
};

const game: GameView = {
	id: "test-game",
	name: "Test Game",
	origin: "bundled",
	verified: null,
	steam_app_ids: [],
	places: [],
	loader: null,
	needs_settings_dir: false,
	needs_mods_root: false,
	providers: [],
};

function row(status: "added" | "found") {
	return renderToString(
		<GameRow
			entry={{ game, status, root: "/games/test", ready: status === "added" }}
			onAdd={() => Promise.resolve(true)}
			onSelect={nothing}
			onOpen={nothing}
			onCopy={nothing}
			onRemove={nothing}
			onLocate={nothing}
			onEdit={nothing}
		/>,
	);
}

test("an added game opens from its name, like its tile in the rail", () => {
	const html = row("added");
	expect(html).toContain('title="Open Test Game"');
	expect(html).toContain("Test Game");
});

test("a game that is not added keeps its name as plain text", () => {
	expect(row("found")).not.toContain('title="Open Test Game"');
});

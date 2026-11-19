import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { GameDataUsage, GameView } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applyLanguage } from "@/i18n";
import { StorageView } from "@/views/storage/storage-page";

await applyLanguage("en");

const game: GameView = {
	id: "slot",
	name: "Slot Game",
	origin: "bundled",
	verified: null,
	steam_app_ids: [],
	places: [],
	loader: null,
	needs_settings_dir: false,
	needs_mods_root: false,
	providers: [],
};

const page = (element: React.ReactElement) => renderToString(<TooltipProvider>{element}</TooltipProvider>);

const doNothing = () => Promise.resolve(true);
const openNothing = () => {
	// Opening a folder is not part of this check.
};

test("a removed game shows its size and the restore and delete actions", () => {
	const usage: GameDataUsage[] = [{ id: game.id, path: "/data/games/slot.removed", removed: true, size_kb: 2048 }];
	const html = page(
		<StorageView
			games={[game]}
			usage={usage}
			loading={false}
			failed={false}
			locale="en"
			onRefresh={doNothing}
			onRestore={doNothing}
			onDelete={doNothing}
			onOpen={openNothing}
		/>,
	);

	expect(html).toContain("Slot Game");
	expect(html).toContain("Removed");
	expect(html).toContain("2.0 MB");
	expect(html).toContain("Restore");
	// The delete action lives in the row menu, which is closed until it is opened.
	expect(html).toContain('aria-label="More actions"');
});

test("no data folders show the empty state", () => {
	const html = page(
		<StorageView
			games={[]}
			usage={[]}
			loading={false}
			failed={false}
			locale="en"
			onRefresh={doNothing}
			onRestore={doNothing}
			onDelete={doNothing}
			onOpen={openNothing}
		/>,
	);

	expect(html).toContain("No game data yet");
});

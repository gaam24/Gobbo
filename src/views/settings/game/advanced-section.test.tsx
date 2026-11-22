import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { GameView } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applyLanguage } from "@/i18n";
import { DefinitionFacts } from "@/views/settings/game/advanced-section";

await applyLanguage("en");

function game(loader: GameView["loader"]): GameView {
	return {
		id: "example-game",
		name: "Example Game",
		origin: "bundled",
		verified: null,
		steam_app_ids: [1234567],
		places: ["mods/plugins"],
		loader,
		needs_settings_dir: false,
		needs_mods_root: false,
		providers: [],
	};
}

// The definition facts are inside a fold that starts closed, so the test renders the facts directly.
function render(loader: GameView["loader"]) {
	return renderToString(
		<TooltipProvider>
			<DefinitionFacts game={game(loader)} />
		</TooltipProvider>,
	);
}

test("the definition shows the loader name, id and variant", () => {
	const html = render({
		id: "example-loader",
		name: "Example Loader",
		variant: "5",
		default_variant: true,
	});
	expect(html).toContain("Example Loader");
	expect(html).toContain("example-loader");
	expect(html).toContain("Loader variant");
	expect(html).toContain("default");
});

test("a game without a loader says so", () => {
	const html = render(null);
	expect(html).toContain("None");
	expect(html).not.toContain("Loader variant");
});

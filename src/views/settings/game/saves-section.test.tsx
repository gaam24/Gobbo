import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { UserDataView } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applyLanguage } from "@/i18n";
import { statusFixture } from "@/test/fixtures/status";
import { SavesSection } from "@/views/settings/game/saves-section";

await applyLanguage("en");

const redirect: UserDataView = {
	id: "documents",
	kind: "saves",
	path: "C:\\Users\\Player\\Documents\\Game",
	mode: "redirect",
	isolated: true,
	inherit: true,
	upper: "C:\\Data\\game\\state\\p1\\userdata\\documents\\upper",
	upper_exists: true,
	available: true,
};

const nothing = () => {
	// The action is not part of this check.
};

function render(set: UserDataView, options: { gameRunning?: boolean; deployed?: boolean } = {}) {
	const status = statusFixture(
		{ kind: "ready", enabled: 0, total: 0 },
		{
			deployed: options.deployed ?? false,
			profiles: [
				{
					id: "p1",
					name: "Default",
					mod_count: 0,
					enabled_count: 0,
					active: true,
					deployed: false,
					has_user_data: true,
					collections: [],
				},
			],
			user_data: [set],
		},
	);
	return renderToString(
		<TooltipProvider>
			<SavesSection
				status={status}
				userData={{
					onSetUserData: nothing,
					onUserDataSize: () => Promise.resolve(2048),
					onMoveUserData: nothing,
					gameRunning: options.gameRunning ?? false,
				}}
				onOpen={nothing}
				profileData={[]}
				onAddProfileData={nothing}
				onRemoveProfileData={nothing}
			/>
		</TooltipProvider>,
	);
}

test("an isolated redirect offers the move back into the game folder", () => {
	const html = render(redirect);
	expect(html).toContain("Move data back to the game folder");
	expect(html).toContain("Saves");
});

test("a running game disables the move and says why", () => {
	const html = render(redirect, { gameRunning: true });
	expect(html).toContain("Close the game before you move its data.");
	expect(html).toContain('disabled=""');
});

test("an active apply explains that it keeps using the layer", () => {
	const html = render(redirect, { deployed: true });
	expect(html).toContain("The list stays applied with this profile");
});

test("a copied set offers no move back", () => {
	const html = render({ ...redirect, mode: "copy" });
	expect(html).not.toContain("Move data back to the game folder");
	expect(html).toContain("copies it in before the game starts");
});

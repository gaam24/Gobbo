import { afterAll, expect, mock, test } from "bun:test";
import type { ComponentProps, ReactNode } from "react";
import { renderToString } from "react-dom/server";

import type { Found, GameView } from "@/bindings";
import { applyLanguage } from "@/i18n";

// A Radix menu mounts its content in a portal only after an open, which a server render cannot do. Draw the items
// inline and keep their handlers, so the checks can read every install and trigger the action it offers.
const inline = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
const menuSelects: Array<() => void> = [];
const menuItem = ({ children, onSelect }: { children?: ReactNode; onSelect?: () => void }) => {
	if (onSelect) menuSelects.push(onSelect);
	return <div>{children}</div>;
};
const buttonClicks: Array<() => void> = [];
const button = ({
	children,
	onClick,
	variant: _variant,
	size: _size,
	asChild: _asChild,
	...props
}: Omit<ComponentProps<"button">, "onClick"> & { onClick?: () => void; variant?: string; size?: string; asChild?: boolean }) => {
	if (onClick) buttonClicks.push(onClick);
	return (
		<button type="button" {...props} onClick={onClick}>
			{children}
		</button>
	);
};

await mock.module("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: inline,
	DropdownMenuCheckboxItem: inline,
	DropdownMenuContent: inline,
	DropdownMenuGroup: inline,
	DropdownMenuItem: menuItem,
	DropdownMenuLabel: inline,
	DropdownMenuPortal: inline,
	DropdownMenuRadioGroup: inline,
	DropdownMenuRadioItem: inline,
	DropdownMenuSeparator: inline,
	DropdownMenuShortcut: inline,
	DropdownMenuSub: inline,
	DropdownMenuSubContent: inline,
	DropdownMenuSubTrigger: inline,
	DropdownMenuTrigger: inline,
}));
await mock.module("@/components/ui/button", () => ({ Button: button }));

await applyLanguage("en");

const { GameRow } = await import("@/views/games/game-row");

afterAll(() => mock.restore());

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

const otherInstall: Found = { game_root: "/games/other", library: "/library/gog", store: "gog" };

function row(status: "added" | "found", shown = game, store: string | null = null, others: Found[] = []) {
	menuSelects.length = 0;
	buttonClicks.length = 0;
	return renderToString(
		<GameRow
			entry={{ game: shown, status, root: "/games/test", ready: status === "added", store, others }}
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

/// A found row with a chosen root, recording which installs the add action was asked for.
function found(others: Found[] = []) {
	menuSelects.length = 0;
	buttonClicks.length = 0;
	const calls: Array<[string, string]> = [];
	const html = renderToString(
		<GameRow
			entry={{ game, status: "found", root: "/games/preferred", ready: false, store: "steam", others }}
			onAdd={(id, gameRoot) => {
				calls.push([id, gameRoot]);
				return Promise.resolve(true);
			}}
			onSelect={nothing}
			onOpen={nothing}
			onCopy={nothing}
			onRemove={nothing}
			onLocate={nothing}
			onEdit={nothing}
		/>,
	);
	return { html, calls };
}

test("an added game opens from its name, like its tile in the rail", () => {
	const html = row("added");
	expect(html).toContain('title="Open Test Game"');
	expect(html).toContain("Test Game");
});

test("a game that is not added keeps its name as plain text", () => {
	expect(row("found")).not.toContain('title="Open Test Game"');
});

test("a downloaded game shows the catalog badge", () => {
	const official: GameView = { ...game, origin: "official" };
	expect(row("added", official)).toContain("From the catalog");
});

test("a bundled game shows no catalog badge", () => {
	expect(row("added")).not.toContain("From the catalog");
});

test("a game from a known store shows its store badge", () => {
	expect(row("added", game, "gog")).toContain("GOG");
	expect(row("added", game, "manual")).toContain("Manual");
});

test("a game without a known store shows no store badge", () => {
	expect(row("added")).not.toContain("GOG");
	expect(row("added", game, "unknown-store")).not.toContain("GOG");
});

test("a game found in several stores offers every install and adds the one you pick", () => {
	const others = [otherInstall, { game_root: "/games/third", library: "/library/epic", store: "epic" }];
	const { html, calls } = found(others);
	expect(html).toContain("Steam");
	expect(html).toContain("/games/preferred");
	expect(html).toContain("GOG");
	expect(html).toContain("/games/other");
	expect(html).toContain("Epic");
	expect(html).toContain("/games/third");

	// The add menu is drawn before the row's more menu, so its items are the first selectors recorded.
	for (const select of menuSelects.slice(0, 1 + others.length)) select();
	expect(calls).toEqual([
		["test-game", "/games/preferred"],
		["test-game", "/games/other"],
		["test-game", "/games/third"],
	]);
});

test("a game found once adds the preferred install without a menu", () => {
	const { html, calls } = found();
	expect(html).toContain("Add");
	expect(html).toContain("/games/preferred");
	expect(buttonClicks).toHaveLength(1);
	buttonClicks[0]?.();
	expect(calls).toEqual([["test-game", "/games/preferred"]]);
});

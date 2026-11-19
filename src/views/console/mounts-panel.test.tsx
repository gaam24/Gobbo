import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";
import type { MountDiagnostics } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { MountsView } from "@/views/console/mounts-panel";

await applyLanguage("en");

const diagnostics: MountDiagnostics = {
	mounts: [
		{
			id: "game",
			guest: "/games/Slot",
			upper: "/data/games/slot/state/p1/overwrite",
			active: false,
		},
		{
			id: "documents",
			guest: "/home/player/Documents/Slot",
			upper: "/data/games/slot/state/p1/userdata/documents/upper",
			active: true,
		},
	],
	warnings: [
		{
			code: "diagnostics.writes_outside",
			params: { count: "3", path: "/games/Slot" },
		},
	],
	orphans: [
		{
			path: "/data/games/slot/state/gone/userdata/documents/upper",
			reason: "profile",
			size_bytes: 2048,
		},
	],
};

const nothing = () => {
	// Actions are not part of a server-rendered structure check.
};

test("the mounts panel names each mount, its layer and whether the game holds it", () => {
	const html = renderToString(<MountsView view={diagnostics} selected={new Set()} onToggle={nothing} onClean={nothing} />);
	expect(html).toContain("Active mounts");
	expect(html).toContain("/games/Slot");
	expect(html).toContain("/data/games/slot/state/p1/overwrite");
	expect(html).toContain("game running");
});

test("a warning about writes outside Gobbo is translated with its values", () => {
	const html = renderToString(<MountsView view={diagnostics} selected={new Set()} onToggle={nothing} onClean={nothing} />);
	expect(html).toContain("Writes outside Gobbo");
	expect(html).toContain("wrote 3 files into the game folder");
});

test("the cleanup button is enabled only when a layer is selected", () => {
	const empty = renderToString(<MountsView view={diagnostics} selected={new Set()} onToggle={nothing} onClean={nothing} />);
	const chosen = renderToString(
		<MountsView view={diagnostics} selected={new Set([diagnostics.orphans[0]?.path ?? ""])} onToggle={nothing} onClean={nothing} />,
	);
	expect(empty).toContain("deleted profile");
	expect(empty).toContain("Clean selected layers");
	expect(empty).toContain('disabled=""');
	expect(chosen).not.toContain('disabled=""');
});

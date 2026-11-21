import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";
import { CloseDialogChoice, type CloseRequest } from "@/app/shell/close-dialog-view";
import { applyLanguage } from "@/i18n";

await applyLanguage("en");

const nothing = () => {
	// The handlers are not part of this structure check.
};

function view(request: CloseRequest, remember = false) {
	return renderToString(<CloseDialogChoice request={request} remember={remember} onRemember={nothing} onChoose={nothing} />);
}

test("the closing dialog offers the tray first, then minimize and exit", () => {
	const html = view({ tray: true, game_running: false });
	const tray = html.indexOf("Hide in the tray");
	const minimize = html.indexOf("Minimize");
	const exit = html.indexOf("Exit Gobbo");
	expect(tray).toBeGreaterThan(-1);
	expect(minimize).toBeGreaterThan(tray);
	expect(exit).toBeGreaterThan(minimize);
	expect(html).toContain("Remember my choice");
});

test("without a tray icon the tray choice is not offered", () => {
	const html = view({ tray: false, game_running: false });
	expect(html).not.toContain("Hide in the tray");
	expect(html).toContain("Minimize");
	expect(html).toContain("Exit Gobbo");
});

test("exiting says that a running game is stopped first", () => {
	expect(view({ tray: true, game_running: true })).toContain("The running game is stopped first.");
	expect(view({ tray: true, game_running: false })).not.toContain("The running game is stopped first.");
});

test("the remember box is shown checked when the choice should be kept", () => {
	expect(view({ tray: true, game_running: false }, true)).toContain('data-state="checked"');
});

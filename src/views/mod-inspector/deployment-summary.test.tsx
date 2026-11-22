import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { gamePresentation } from "@/app/selectors/game-presentation";
import type { Status_Serialize } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { modFixture } from "@/test/fixtures/mod";
import { statusFixture } from "@/test/fixtures/status";
import { DeploymentSummary } from "@/views/mod-inspector/deployment-summary";

await applyLanguage("en");

function render(status: Status_Serialize): string {
	return renderToString(
		<DeploymentSummary
			gameName="Example Game"
			status={status}
			updates={{}}
			gameRunning={false}
			busy={null}
			onImport={() => undefined}
			onDeploy={() => undefined}
			onOpenDownloads={() => undefined}
			onInstallLoader={() => undefined}
		/>,
	);
}

test("a blocked list does not offer a deploy action the header does not take", () => {
	const status = statusFixture(
		{ kind: "blocked", reasons: [{ code: "status.requirement_missing", params: {} }] },
		{ next_action: "fix_problems", blocker: "problems", mods: [modFixture({ id: "local:enabled" })] },
	);
	const presentation = gamePresentation(status);
	expect(presentation.nextStep?.action.id).toBe("fix-problems");
	expect(presentation.primary.id).toBe("fix-problems");

	const html = render(status);
	expect(html).not.toContain("Review and apply");
	expect(html).toContain("Apply the mods and start the game");
});

test("an all-disabled list leaves the enable step to the header without a deploy button", () => {
	const status = statusFixture(
		{ kind: "no_enabled", total: 2 },
		{
			next_action: "enable_mods",
			blocker: "no_mods",
			mods: [modFixture({ id: "local:off", enabled: false }), modFixture({ id: "local:other", enabled: false })],
		},
	);
	const presentation = gamePresentation(status);
	expect(presentation.nextStep?.titleKey).toBe("start.enable.title");
	expect(presentation.primary).toMatchObject({ id: "play", labelKey: "header.playWithoutMods" });

	const html = render(status);
	expect(html).toContain("Enable the mods you want");
	expect(html).toContain('aria-current="step"');
	// The card owns none of the actions of this step: enabling happens in the list, and playing in the header.
	expect(html).not.toContain("Review and apply");
	expect(html).not.toContain("Get mods online");
});

test("a fix-problems step with enabled mods highlights fixing, not deploying", () => {
	const status = statusFixture(
		{ kind: "blocked", reasons: [{ code: "status.requirement_missing", params: {} }] },
		{ next_action: "fix_problems", blocker: "problems", mods: [modFixture({ id: "local:enabled" })] },
	);
	expect(gamePresentation(status).nextStep?.action.id).toBe("fix-problems");

	const html = render(status);
	expect(html).toContain("Fix the problems in the list");
	expect(html).toContain("A few mods need attention before they can be applied.");
	expect(html).not.toContain("Review and apply");
});

/// The apply button lives in the draft card; read its tag from the rendered markup to check the disabled attribute.
function draftApplyButton(html: string): string | null {
	const at = html.indexOf("Review and apply");

	if (at < 0) return null;
	const start = html.lastIndexOf("<button", at);
	return html.slice(start, html.indexOf(">", start) + 1);
}

test("a blocked draft disables the apply button the header would refuse", () => {
	const status = statusFixture(
		{ kind: "draft", changes: 2, enabled: 1, total: 1 },
		{ deployed: true, draft_changes: 2, next_action: "apply", blocker: "problems", mods: [modFixture({ id: "local:enabled" })] },
	);
	const presentation = gamePresentation(status);
	// The draft stage keeps `canApply: true`; the blocker is what makes the button dead.
	expect(presentation.canApply).toBe(true);
	expect(presentation.blockedReason).toBe("deploy.blocker.problems");

	const button = draftApplyButton(render(status));
	expect(button).not.toBeNull();
	expect(button).toContain('disabled=""');
});

test("an unblocked draft offers an enabled apply button", () => {
	const status = statusFixture(
		{ kind: "draft", changes: 2, enabled: 1, total: 1 },
		{ deployed: true, draft_changes: 2, next_action: "apply", mods: [modFixture({ id: "local:enabled" })] },
	);
	const button = draftApplyButton(render(status));
	expect(button).not.toBeNull();
	expect(button).not.toContain('disabled=""');
});

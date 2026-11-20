import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { ImportVersionBanner } from "@/views/mods/import-version-banner";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

test("the imported version banner names the mod and offers the choice", () => {
	const html = renderToString(
		<ImportVersionBanner
			mods={[{ family: "mod-id", sha256: "sha", folder: "Mod 2.0", name: "Mod", version: "2.0" }]}
			onUse={() => {
				// The action is not part of the structure check.
			}}
			onDismiss={() => {
				// The action is not part of the structure check.
			}}
		/>,
	);
	expect(html).toContain("A new version was imported");
	expect(html).toContain("2.0");
	expect(html).toContain("Use this version");
});

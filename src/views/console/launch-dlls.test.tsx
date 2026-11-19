import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { LaunchDlls } from "@/views/console/launch-dlls";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

test("the console separates injected static imports from the DLLs found in the layer", () => {
	const html = renderToString(
		<LaunchDlls
			dlls={{
				layer: ["bin/x64/winmm.dll", "red4ext/RED4ext.dll"],
				injected: [
					{
						name: "WINMM.dll",
						path: "bin/x64/winmm.dll",
						source: "/example/mods/bin/x64/winmm.dll",
					},
				],
			}}
		/>,
	);
	expect(html).toContain("Mod DLLs the game loaded");
	expect(html).toContain("WINMM.dll");
	expect(html).toContain("red4ext/RED4ext.dll");
	expect(html).toContain("1 static import patched");
});

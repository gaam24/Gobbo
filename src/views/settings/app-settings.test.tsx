import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";
import { NavigationProvider } from "@/app/contexts/navigation-context";
import { UnsavedProvider } from "@/app/contexts/unsaved-context";
import type { SourceProviderInfo } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applyLanguage } from "@/i18n";
import { AppSettings } from "@/views/settings/app-settings";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const nothing = () => {
	// The row actions are not part of this check.
};

const nexus: SourceProviderInfo = {
	id: "nexus",
	name: "Nexus",
	needs_key: true,
	has_key_page: true,
	direct_download: true,
	download_route: "direct",
	collections: false,
	schemes: ["nxm"],
};

/// The settings with one provider whose links the system sends to another program.
function settings(registered: boolean) {
	return (
		<UnsavedProvider>
			<NavigationProvider>
				<TooltipProvider>
					<AppSettings
						language="en"
						onLanguage={nothing}
						providers={{
							list: [nexus],
							keysSaved: {},
							accounts: {},
							links: ["nexus"],
							status: [{ provider: "nexus", scheme: "nxm", registered }],
						}}
						providerActions={{
							onSaveKey: () => Promise.resolve(true),
							onValidateKey: () => Promise.resolve({ ok: true, text: "" }),
							onOpenKeyPage: nothing,
							onLinks: nothing,
							onRetakeLinks: nothing,
						}}
						rememberWindow
						onRememberWindow={nothing}
						minimizeToTray
						onMinimizeToTray={nothing}
						efficiencyMode="auto"
						onEfficiencyMode={nothing}
						checkCollectionsOnLaunch
						onCheckCollectionsOnLaunch={nothing}
						dataFolder="/tmp/gobbo"
						storage={null}
						onOpenFolder={nothing}
						onCopy={nothing}
						gameId="test"
					/>
				</TooltipProvider>
			</NavigationProvider>
		</UnsavedProvider>
	);
}

test("a link another program handles shows the warning and the take-back action", () => {
	const html = renderToString(settings(false));
	expect(html).toContain("Download links don&#x27;t open in Gobbo");
	expect(html).toContain("nxm://");
	expect(html).toContain("Get the links back");
});

test("a link Gobbo handles shows no warning", () => {
	const html = renderToString(settings(true));
	expect(html).not.toContain("Download links don&#x27;t open in Gobbo");
	expect(html).not.toContain("Get the links back");
});

test("only the selected settings tab is visible", () => {
	const html = renderToString(settings(true));
	expect(html).toContain("Remember the window");
	expect(html).not.toContain("Create support bundle");
});

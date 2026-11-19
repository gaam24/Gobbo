import { describe, expect, test } from "bun:test";
import type { SourceProviderInfo } from "@/bindings";
import { browserProviders } from "@/sources/access";

const provider = (id: string, route: SourceProviderInfo["download_route"]): SourceProviderInfo => ({
	id,
	name: id,
	needs_key: true,
	has_key_page: true,
	direct_download: true,
	download_route: route,
	collections: false,
	schemes: [],
});

describe("browserProviders", () => {
	test("keeps exactly the providers the backend routes to the browser", () => {
		const browser = browserProviders([provider("nexus", "browser"), provider("thunderstore", "direct")]);
		expect(browser.has("nexus")).toBe(true);
		expect(browser.has("thunderstore")).toBe(false);
	});

	test("an empty list routes nothing to the browser", () => {
		expect(browserProviders([]).size).toBe(0);
	});
});

import { describe, expect, test } from "bun:test";
import type { SourceProviderInfo } from "@/bindings";
import { anyReady, readyProviders } from "@/sources/ready";

const provider = (id: string, needsKey: boolean): SourceProviderInfo => ({
	id,
	name: id,
	needs_key: needsKey,
	has_key_page: needsKey,
	direct_download: true,
	download_route: "direct",
	collections: false,
	schemes: [],
});

describe("readyProviders", () => {
	const providers = [provider("nexus", true), provider("thunderstore", false)];

	test("a provider without keys is ready without any saved key", () => {
		const ready = readyProviders(providers, {});
		expect(ready.has("thunderstore")).toBe(true);
		expect(ready.has("nexus")).toBe(false);
	});

	test("a provider with a key is ready once its key is saved", () => {
		expect(readyProviders(providers, { nexus: true }).has("nexus")).toBe(true);
		expect(readyProviders(providers, { nexus: false }).has("nexus")).toBe(false);
	});

	test("a game or a mod can be checked when any of its providers is ready", () => {
		const ready = readyProviders(providers, {});
		expect(anyReady(ready, ["thunderstore"])).toBe(true);
		expect(anyReady(ready, ["nexus"])).toBe(false);
		expect(anyReady(ready, [])).toBe(false);
		expect(anyReady(ready, ["unknown"])).toBe(false);
	});
});

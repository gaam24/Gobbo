import { describe, expect, test } from "bun:test";
import { lostLinkProviders, lostLinkSchemes } from "@/sources/links";

const status = (provider: string, scheme: string, registered: boolean) => ({
	provider,
	scheme,
	registered,
});

describe("lost link handlers", () => {
	test("a registered scheme names no provider", () => {
		expect(lostLinkProviders([status("nexus", "nxm", true)])).toEqual([]);
		expect(lostLinkSchemes([status("nexus", "nxm", true)])).toEqual([]);
	});

	test("a lost scheme names its provider once, even when the provider has several schemes", () => {
		const statuses = [status("nexus", "nxm", false), status("nexus", "nxm-alt", false), status("thunderstore", "ror2mm", true)];
		expect(lostLinkProviders(statuses)).toEqual(["nexus"]);
		expect(lostLinkSchemes(statuses)).toEqual(["nxm://", "nxm-alt://"]);
	});

	test("every provider the settings take is reported", () => {
		const statuses = [status("nexus", "nxm", false), status("thunderstore", "ror2mm", false)];
		expect(lostLinkProviders(statuses)).toEqual(["nexus", "thunderstore"]);
		expect(lostLinkSchemes(statuses)).toEqual(["nxm://", "ror2mm://"]);
	});
});

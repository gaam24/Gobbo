import { describe, expect, test } from "bun:test";
import type { DownloadView_Serialize } from "@/bindings";
import { matchesUpdateDownload, splitUpdateTargets, type UpdateTarget } from "@/sources/update-targets";

const target = (overrides: Partial<UpdateTarget> = {}): UpdateTarget => ({
	id: "nexus:1",
	name: "Example",
	folder: "Example 1.0",
	provider: "nexus",
	modId: "7",
	current: "1.0",
	latest: "2.0",
	latestFileId: "200",
	installed: false,
	...overrides,
});

const download = (overrides: Partial<DownloadView_Serialize> = {}): DownloadView_Serialize => ({
	id: 1,
	game: "game",
	name: "Example 2.0",
	source: { provider: "nexus", mod_id: "7", file_id: "200" },
	state: "downloading",
	received_kb: 0,
	total_kb: 0,
	speed_kb: 0,
	path: null,
	auto_install: true,
	error: null,
	...overrides,
});

describe("splitUpdateTargets", () => {
	test("the provider route decides even when the newest version is already installed", () => {
		const split = splitUpdateTargets([target({ installed: true }), target({ id: "nexus:2" })], new Set(["nexus"]));
		expect(split.direct).toEqual([]);
		expect(split.manual.map((item) => item.id)).toEqual(["nexus:1", "nexus:2"]);
	});

	test("a direct provider puts every target in the direct run, installed or not", () => {
		const split = splitUpdateTargets([target({ installed: true }), target({ id: "thunderstore:1", provider: "thunderstore" })], new Set());
		expect(split.direct).toHaveLength(2);
		expect(split.manual).toEqual([]);
	});
});

describe("matchesUpdateDownload", () => {
	test("the newest provider file matches its queued download", () => {
		expect(matchesUpdateDownload(target(), download(), "game")).toBe(true);
	});

	test("another file of the same mod does not match", () => {
		expect(matchesUpdateDownload(target(), download({ source: { provider: "nexus", mod_id: "7", file_id: "100" } }), "game")).toBe(false);
	});

	test("a provider without file ids matches on the mod", () => {
		expect(matchesUpdateDownload(target({ latestFileId: null }), download({ source: { provider: "nexus", mod_id: "7" } }), "game")).toBe(
			true,
		);
	});

	test("a download of another game or provider does not match", () => {
		expect(matchesUpdateDownload(target(), download({ game: "other" }), "game")).toBe(false);
		expect(matchesUpdateDownload(target(), download({ source: { provider: "thunderstore", mod_id: "7", file_id: "200" } }), "game")).toBe(
			false,
		);
	});
});

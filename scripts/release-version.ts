/// <reference types="bun" />
import { readFileSync } from "node:fs";
import { join } from "node:path";

/// The one accessor for the app version. `release.json` owns it; the release-meta guard and the updater tests both read
/// through here, so a bump needs no edits to the tests and no version literal can drift into them.
export function releaseVersion(): string {
	const meta: unknown = JSON.parse(readFileSync(join(import.meta.dir, "..", "release.json"), "utf8"));

	if (typeof meta === "object" && meta !== null && "version" in meta && typeof meta.version === "string") {
		return meta.version;
	}

	throw new Error("release.json has no version");
}

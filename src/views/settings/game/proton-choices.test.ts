import { expect, test } from "bun:test";

import type { ProtonBuild } from "@/bindings";
import { protonChoices } from "@/views/settings/game/proton-choices";

const ge: ProtonBuild = {
	name: "GE-Proton9-20",
	folder: "/steam/compatibilitytools.d/GE-Proton9-20",
};
const proton: ProtonBuild = {
	name: "Proton 9.0",
	folder: "/library/steamapps/common/Proton 9.0",
};
const builds = [ge, proton];

test("Steam's choice lists the detected builds as they are", () => {
	expect(protonChoices(builds, "")).toEqual(builds);
});

test("a chosen folder that is not detected stays in the list, first", () => {
	const custom = "/opt/Proton-custom";
	expect(protonChoices(builds, custom)).toEqual([{ name: custom, folder: custom }, ...builds]);
});

test("a chosen folder that is detected is listed once", () => {
	expect(protonChoices(builds, proton.folder)).toEqual(builds);
});

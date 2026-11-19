import { expect, test } from "bun:test";

import { isProtonOption, PROTON_OPTION_GROUPS } from "@/views/settings/game/proton-options";

test("every option is shown in exactly one group", () => {
	const grouped = PROTON_OPTION_GROUPS.flatMap((group) => group.options);
	expect(new Set(grouped).size).toBe(grouped.length);
});

test("only the options a group shows are known to the interface", () => {
	expect(["wined3d", "a_future_switch"].filter((id) => isProtonOption(id))).toEqual(["wined3d"]);
	expect(["antilag", "low_latency"].filter((id) => isProtonOption(id))).toEqual(["antilag", "low_latency"]);
});

import { expect, test } from "bun:test";

import { freeName } from "@/lib/profile-names";

const profile = (number: number) => `Profile ${number}`;

test("the first profile of an empty list is number one", () => {
	expect(freeName(profile, [])).toBe("Profile 1");
});

test("a taken number is skipped, whatever its case and spaces", () => {
	expect(freeName(profile, ["Profile 1", "  profile 2 "])).toBe("Profile 3");
});

test("a gap in the numbers is filled", () => {
	expect(freeName(profile, ["Default", "Profile 2"])).toBe("Profile 1");
});

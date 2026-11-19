import { expect, test } from "bun:test";

import { orderedColumns } from "@/lib/mod-columns";
import { FIT_MAX, FIT_SAMPLE, sampleRows, trackList } from "@/views/mod-table/layout";

test("a short list is measured whole", () => {
	const rows = [0, 1, 2, 3];
	expect(sampleRows(rows, 5)).toBe(rows);
	expect(sampleRows([], 5)).toEqual([]);
});

test("a long list is sampled evenly without duplicates", () => {
	const rows = Array.from({ length: 10_000 }, (_, index) => index);
	const picked = sampleRows(rows);

	expect(picked).toHaveLength(FIT_SAMPLE);
	expect(picked[0]).toBe(0);
	expect(picked.at(-1)).toBeGreaterThan(9_000);
	expect(new Set(picked).size).toBe(FIT_SAMPLE);
	expect(picked).toEqual(picked.toSorted((left, right) => left - right));
});

test("a limit of one keeps the first row", () => {
	expect(sampleRows([10, 20, 30], 1)).toEqual([10]);
});

test("the flexible mod column stops at the fitted width and a saved width wins", () => {
	const columns = orderedColumns(["mod", "version"]);
	expect(trackList(columns, {})).toBe(`28px minmax(200px,${FIT_MAX}px) 168px`);
	expect(trackList(columns, { mod: 900 })).toBe("28px 900px 168px");
});

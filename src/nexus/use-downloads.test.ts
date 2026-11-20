import { expect, test } from "bun:test";

import { type Download, downloadSummary } from "@/nexus/use-downloads";

function download(overrides: Partial<Download>): Download {
	return {
		id: 1,
		game: "examplegame",
		name: "CET",
		source: null,
		state: "downloading",
		received_kb: 0,
		total_kb: 0,
		speed_kb: 0,
		path: null,
		auto_install: true,
		error: null,
		...overrides,
	};
}

test("the rail adds up the running downloads and counts the ones that wait", () => {
	expect(downloadSummary([])).toEqual({
		running: 0,
		percent: null,
		waiting: 0,
		failed: 0,
	});
	expect(
		downloadSummary([
			download({ id: 1, received_kb: 50, total_kb: 100 }),
			download({ id: 2, received_kb: 0, total_kb: 300 }),
			download({ id: 3, state: "ready" }),
			download({ id: 4, state: "installed" }),
			download({ id: 5, state: "installing" }),
			download({ id: 6, state: "failed" }),
		]),
	).toEqual({ running: 3, percent: 12, waiting: 1, failed: 1 });
	// A download that has not said its size yet shows no bar.
	expect(downloadSummary([download({ received_kb: 10 })]).percent).toBeNull();
	// With one known and one unknown size, only the known transfer is measured; the unknown one cannot inflate it.
	expect(
		downloadSummary([download({ id: 1, received_kb: 200, total_kb: 0 }), download({ id: 2, received_kb: 0, total_kb: 100 })]).percent,
	).toBe(0);
});

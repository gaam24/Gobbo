import { expect, test } from "bun:test";
import { runBusyAction } from "@/lib/use-busy-action";

test("a successful action clears the old failure and ends not busy", async () => {
	const busy: boolean[] = [];
	let runs = 0;
	let cleared = false;
	let failed = false;
	await runBusyAction(
		() => {
			runs += 1;
			return Promise.resolve();
		},
		(value) => busy.push(value),
		() => {
			failed = true;
		},
		() => {
			cleared = true;
		},
	);
	expect(runs).toBe(1);
	expect(cleared).toBe(true);
	expect(failed).toBe(false);
	expect(busy.at(-1)).toBe(false);
});

test("a failed action is reported and still ends not busy", async () => {
	const busy: boolean[] = [];
	const failure = new Error("failed");
	let reported: unknown = null;
	await runBusyAction(
		() => Promise.reject(failure),
		(value) => busy.push(value),
		(error) => {
			reported = error;
		},
	);
	expect(reported).toBe(failure);
	expect(busy.at(-1)).toBe(false);
});

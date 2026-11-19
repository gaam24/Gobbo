import { expect, test } from "bun:test";
import { runBusyAction } from "@/lib/use-busy-action";

test("a successful action clears the busy flag and the old failure", async () => {
	const calls: string[] = [];
	await runBusyAction(
		() => {
			calls.push("run");
			return Promise.resolve();
		},
		(busy) => calls.push(`busy:${busy}`),
		() => calls.push("error"),
		() => calls.push("clear"),
	);
	expect(calls).toEqual(["busy:true", "clear", "run", "busy:false"]);
});

test("a failed action is reported and still clears the busy flag", async () => {
	const calls: string[] = [];
	const failure = new Error("failed");
	await runBusyAction(
		() => Promise.reject(failure),
		(busy) => calls.push(`busy:${busy}`),
		(error) => {
			expect(error).toBe(failure);
			calls.push("error");
		},
	);
	expect(calls).toEqual(["busy:true", "error", "busy:false"]);
});

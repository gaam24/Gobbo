import { expect, test } from "bun:test";
import { answerState, isCurrentReply } from "@/lib/use-load";

test("a repeated answer keeps the state object, so a poll that finds nothing new does not render again", () => {
	const first = answerState(null, "game", true);
	expect(answerState(first, "game", true)).toBe(first);
});

test("a changed value or another key replaces the state", () => {
	const first = answerState(null, "game", true);
	expect(answerState(first, "game", false)).toEqual({ key: "game", value: false });
	expect(answerState(first, "other", true)).toEqual({ key: "other", value: true });
});

test("an object answer is a new state each time, because it is a new object", () => {
	const first = answerState<{ n: number } | null>(null, "game", { n: 1 });
	expect(answerState(first, "game", { n: 1 })).not.toBe(first);
});

test("a reply is dropped after cleanup or after a newer request starts", () => {
	expect(isCurrentReply(true, 2, 2)).toBe(true);
	expect(isCurrentReply(false, 2, 2)).toBe(false);
	expect(isCurrentReply(true, 3, 2)).toBe(false);
});

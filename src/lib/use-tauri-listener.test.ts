import { expect, test } from "bun:test";
import { createListenerScope, type Listen } from "@/lib/use-tauri-listener";

/// The real `listen` needs the Tauri runtime; the scope only keeps it to hand to callers, so a stub is enough.
/// No native subscription needs stopping, so the stub returns an empty unlisten.
const noop = () => {
	// Nothing to stop.
};
function stubListen(): Listen {
	return () => Promise.resolve(noop);
}

test("registered unlistens all run when the scope closes", () => {
	const { scope, close } = createListenerScope(stubListen());
	const stopped: number[] = [];
	scope.add(
		() => stopped.push(1),
		() => stopped.push(2),
	);
	close();
	expect(stopped).toEqual([1, 2]);
});

test("an unlisten that arrives after cleanup runs at once", () => {
	const { scope, close } = createListenerScope(stubListen());
	close();
	let stopped = 0;
	scope.add(() => {
		stopped += 1;
	});
	expect(stopped).toBe(1);
});

test("active turns false on close, so a late answer can skip its work", () => {
	const { scope, close } = createListenerScope(stubListen());
	expect(scope.active).toBe(true);
	close();
	expect(scope.active).toBe(false);
});

test("closing twice never stops the same listener twice", () => {
	const { scope, close } = createListenerScope(stubListen());
	let stopped = 0;
	scope.add(() => {
		stopped += 1;
	});
	close();
	close();
	expect(stopped).toBe(1);
});

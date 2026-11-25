import { expect, test } from "bun:test";
import { cleanPath, relativeToFolder } from "@/views/games/editor/game-path";

test("a pasted path loses its surrounding spaces and one pair of quotes", () => {
	expect(cleanPath('  "C:\\Games\\Example Game"  ')).toBe("C:\\Games\\Example Game");
	expect(cleanPath("C:\\Games\\Example's Game")).toBe("C:\\Games\\Example's Game");
	expect(cleanPath('"')).toBe('"');
	expect(cleanPath("")).toBe("");
});

test("a file inside the game folder is named by its path from that folder, with slashes", () => {
	expect(relativeToFolder("C:\\Games\\Example's Game", "C:\\Games\\Example's Game\\bin\\Example.exe")).toBe("bin/Example.exe");
	expect(relativeToFolder('"C:/Games/Example Game/"', "C:\\Games\\Example Game\\game.exe")).toBe("game.exe");
	expect(relativeToFolder("C:\\Games\\Example Game", "c:\\games\\EXAMPLE GAME\\Game.EXE")).toBe("Game.EXE");
	expect(relativeToFolder("/home/player/Example Game", "/home/player/Example Game/game.x86_64")).toBe("game.x86_64");
});

test("a file outside the folder, or in a folder whose name only starts the same, is not inside it", () => {
	expect(relativeToFolder("C:\\Games\\Example Game", "C:\\Games\\Example Game 2\\game.exe")).toBeNull();
	expect(relativeToFolder("C:\\Games\\Example Game", "D:\\Tools\\game.exe")).toBeNull();
	expect(relativeToFolder("C:\\Games\\Example Game", "C:\\Games\\Example Game")).toBeNull();
	expect(relativeToFolder("", "C:\\Games\\game.exe")).toBeNull();
});

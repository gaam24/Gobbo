import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/// The folders the E2E suite owns. `target/` is ignored, so no fixture can reach the repository or the user's library.
export const here = path.dirname(fileURLToPath(import.meta.url));
export const root = path.resolve(here, "../..");
export const dataDir = path.join(root, "target/e2e-data");
export const homeDir = path.join(root, "target/e2e-home");
export const artifacts = path.join(root, "target/e2e-artifacts");
export const appBinary = path.join(root, "src-tauri/target/debug/gobbo");

export const fixturesDir = path.join(dataDir, "fixtures");
export const rootsDir = path.join(dataDir, "roots");
export const outDir = path.join(dataDir, "out");
export const definitionsDir = path.join(dataDir, "definitions/games");
export const loadersDir = path.join(dataDir, "definitions/loaders");

/// The app's environment. The suite sets the same values on the app process and on the Node side, so a path built by a
/// fixture is the path the app sees.
export function appEnvironment(): Record<string, string> {
	const env: Record<string, string> = {
		GOBBO_DATA_DIR: dataDir,
		HOME: homeDir,
		USERPROFILE: homeDir,
		XDG_DATA_HOME: path.join(homeDir, ".local/share"),
		XDG_CONFIG_HOME: path.join(homeDir, ".config"),
		XDG_CACHE_HOME: path.join(homeDir, ".cache"),
		LANG: "C.UTF-8",
		LC_ALL: "C.UTF-8",
	};
	return env;
}

export function applyProcessEnvironment(): void {
	for (const [name, value] of Object.entries(appEnvironment())) {
		process.env[name] = value;
	}
}

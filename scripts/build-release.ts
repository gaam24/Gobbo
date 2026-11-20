import { spawnSync } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// One release build for the current OS: the overlay hook (with its PDBs), the web bundle, the Tauri bundle and the
// portable archive in `dist-release/`. The Windows symbols are checked before packaging: a portable archive without them
// cannot explain a crash from a user.

const root = path.resolve(import.meta.dirname, "..");
const srcTauri = path.join(root, "src-tauri");
const targetDir = path.resolve(srcTauri, process.env.CARGO_TARGET_DIR ?? "target");

function run(command: string, args: string[], cwd = root) {
	const result = spawnSync(command, args, {
		cwd,
		env: process.env,
		stdio: "inherit",
	});

	if (result.status !== 0) {
		process.exit(result.status ?? 1);
	}
}

run("bun", ["run", "build:overlay-hook", "--release"]);
run("bun", ["run", "build:web"]);
// Tauri copies `games/` into the target directory but never removes a folder a rename left behind. Start empty so the
// release run and any later package read one definition per game, under its current id.
rmSync(path.join(targetDir, "release", "games"), { recursive: true, force: true });
// Tauri's build script writes `resource.lib` under `target/release/build`. A `cargo clean -p` of another package can
// remove that folder while Cargo still considers the app fingerprint fresh, and the linker then fails on the missing
// file; remove the app fingerprint so the build script runs again.
if (process.platform === "win32" && !existsSync(path.join(targetDir, "release", "build"))) {
	run("cargo", ["clean", "-p", "gobbo", "--release"], srcTauri);
}
// `--updater` signs the updater archives with TAURI_SIGNING_PRIVATE_KEY, so the release can publish latest.json. A build
// without the key would ship no update path, so it is refused here rather than left to the signer.
const updater = process.argv.includes("--updater");

if (updater && !process.env.TAURI_SIGNING_PRIVATE_KEY) {
	console.error("--updater needs TAURI_SIGNING_PRIVATE_KEY: an unsigned release cannot publish an update manifest.");
	process.exit(1);
}

run("bun", ["run", "tauri", "build", ...(updater ? ["--config", "src-tauri/tauri.updater.conf.json"] : [])]);

if (process.platform === "win32") {
	for (const name of ["gobbo_overlay_hook.pdb", "gobbo_overlay_hook32.pdb"]) {
		const file = path.join(targetDir, "release", name);

		if (!existsSync(file)) {
			console.error(`The release build has no ${name} at ${file}.`);
			process.exit(1);
		}
	}
}

run("bun", ["run", "package"]);

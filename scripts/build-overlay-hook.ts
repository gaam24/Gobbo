import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// Builds the Windows overlay binaries and stages them in two places:
// - `src-tauri/overlay-hook/`, which Tauri bundles as resources (`tauri.windows.conf.json`);
// - `target/<profile>/`, where the app and the overlay tests look for them next to the executable.
// Both architectures are built: a 32-bit game or child process needs the 32-bit hook. The static CRT comes from
// `src-tauri/.cargo/config.toml`, so a later `cargo test` reuses these builds instead of rebuilding the hook.
//
// Needs the 32-bit target: `rustup target add i686-pc-windows-msvc`. Pass `--release` for the release build.

if (process.platform !== "win32") {
	process.stdout.write("The overlay hook is Windows only, nothing to build here.\n");
	process.exit(0);
}

const profile = process.argv.includes("--release") ? "release" : "debug";
const srcTauri = path.resolve(import.meta.dirname, "..", "src-tauri");
const targetDir = path.resolve(srcTauri, process.env.CARGO_TARGET_DIR ?? "target");
const stagedDir = path.join(srcTauri, "overlay-hook");
const native = process.arch === "arm64" ? "aarch64-pc-windows-msvc" : undefined;
const x86 = "i686-pc-windows-msvc";

function cargoBuild(pkg: string, target: string | undefined, bin?: string) {
	const args = ["build", "-p", pkg, ...(bin ? ["--bin", bin] : []), ...(target ? ["--target", target] : [])];
	if (profile === "release") args.push("--release");
	const result = spawnSync("cargo", args, { cwd: srcTauri, stdio: "inherit" });
	if (result.status !== 0) {
		if (target === x86) console.error("The 32-bit build needs: rustup target add i686-pc-windows-msvc");
		process.exit(result.status ?? 1);
	}
}

const out = (target: string | undefined, file: string) => path.join(targetDir, ...(target ? [target] : []), profile, file);

cargoBuild("gobbo-overlay-hook", native);
cargoBuild("gobbo-overlay-hook", x86);
cargoBuild("gobbo-overlay-launch", x86, "gobbo_overlay_inject32");
cargoBuild("gobbo-overlay-probe", native);
cargoBuild("gobbo-overlay-probe", x86);
cargoBuild("gobbo-overlay-dump", x86, "gobbo_overlay_dump32");

// [built file, staged name, optional]. PDBs are optional: a build without debug info has none.
const artifacts: Array<[string, string, boolean]> = [
	[out(native, "gobbo_overlay_hook.dll"), "gobbo_overlay_hook.dll", false],
	[out(native, "gobbo_overlay_hook.pdb"), "gobbo_overlay_hook.pdb", true],
	[out(x86, "gobbo_overlay_hook.dll"), "gobbo_overlay_hook32.dll", false],
	[out(x86, "gobbo_overlay_hook.pdb"), "gobbo_overlay_hook32.pdb", true],
	[out(x86, "gobbo_overlay_inject32.exe"), "gobbo_overlay_inject32.exe", false],
	[out(native, "overlay-probe.exe"), "gobbo_overlay_probe.exe", false],
	[out(x86, "overlay-probe.exe"), "gobbo_overlay_probe32.exe", false],
	[out(x86, "gobbo_overlay_dump32.exe"), "gobbo_overlay_dump32.exe", false],
];

mkdirSync(stagedDir, { recursive: true });
for (const [source, name, optional] of artifacts) {
	if (!existsSync(source)) {
		if (optional) continue;
		console.error(`${name} was not built at ${source}.`);
		process.exit(1);
	}
	copyFileSync(source, path.join(stagedDir, name));
	const besideApp = path.join(targetDir, profile, name);
	if (path.resolve(source) !== path.resolve(besideApp)) copyFileSync(source, besideApp);
}

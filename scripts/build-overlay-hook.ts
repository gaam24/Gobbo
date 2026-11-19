import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// Builds the overlay hook DLLs that Gobbo loads into the games it starts, and stages them under `src-tauri/overlay-hook`
// where Tauri picks them up as bundle resources (see `tauri.windows.conf.json`). Gobbo looks for the DLLs next to its
// own executable, so the dev and release builds both get them. The hook exists on Windows only.
//
// Both architectures are built: a 32-bit game or a 32-bit child process needs `gobbo_overlay_hook32.dll`, and running it
// unhooked could let it write the game folder. The x86 build needs the Rust target:
//     rustup target add i686-pc-windows-msvc
//
// Pass `--release` for the release build; it is used by `beforeBuildCommand`.

if (process.platform !== "win32") {
	process.stdout.write("The overlay hook is Windows only, nothing to build here.\n");
	process.exit(0);
}

const release = process.argv.includes("--release");
const profile = release ? "release" : "debug";
/// PGO flags belong to the app build. The hook is injected into games and has its own profile; a profile-use built
/// from the app benchmark would only produce missing-function warnings here.
const inheritedFlags = (process.env.RUSTFLAGS ?? "")
	.split(/\s+/)
	.filter((part) => part.length > 0 && !part.startsWith("-Cprofile-use") && !part.startsWith("-Cprofile-generate"))
	.join(" ");
const srcTauri = path.resolve(import.meta.dirname, "..", "src-tauri");
const targetDir = path.resolve(srcTauri, process.env.CARGO_TARGET_DIR ?? "target");
const arm64Target = "aarch64-pc-windows-msvc";
const x86Target = "i686-pc-windows-msvc";
const devNative = path.join(targetDir, profile, "gobbo_overlay_hook.dll");
const dev32 = path.join(targetDir, profile, "gobbo_overlay_hook32.dll");
const devPdb = path.join(targetDir, profile, "gobbo_overlay_hook.pdb");
const devPdb32 = path.join(targetDir, profile, "gobbo_overlay_hook32.pdb");
const devInjector = path.join(targetDir, profile, "gobbo_overlay_inject32.exe");
const devProbe = path.join(targetDir, profile, "gobbo_overlay_probe.exe");
const devProbe32 = path.join(targetDir, profile, "gobbo_overlay_probe32.exe");
const devDump32 = path.join(targetDir, profile, "gobbo_overlay_dump32.exe");
const x86Dll = path.join(targetDir, x86Target, profile, "gobbo_overlay_hook.dll");
const x86Pdb = path.join(targetDir, x86Target, profile, "gobbo_overlay_hook.pdb");
const x86Injector = path.join(targetDir, x86Target, profile, "gobbo_overlay_inject32.exe");
const x86Probe = path.join(targetDir, x86Target, profile, "overlay-probe.exe");
const x86Dump32 = path.join(targetDir, x86Target, profile, "gobbo_overlay_dump32.exe");
const stagedDir = path.join(srcTauri, "overlay-hook");
const stagedNative = path.join(stagedDir, "gobbo_overlay_hook.dll");
const staged32 = path.join(stagedDir, "gobbo_overlay_hook32.dll");
const stagedPdb = path.join(stagedDir, "gobbo_overlay_hook.pdb");
const stagedPdb32 = path.join(stagedDir, "gobbo_overlay_hook32.pdb");
const stagedInjector = path.join(stagedDir, "gobbo_overlay_inject32.exe");
const stagedProbe = path.join(stagedDir, "gobbo_overlay_probe.exe");
const stagedProbe32 = path.join(stagedDir, "gobbo_overlay_probe32.exe");
const stagedDump32 = path.join(stagedDir, "gobbo_overlay_dump32.exe");

function build(packageName: string, archName: string, target?: string, bin?: string, staticCrt = false) {
	const args = ["build", "-p", packageName];
	if (bin !== undefined) {
		args.push("--bin", bin);
	}
	if (release) {
		args.push("--release");
	}
	if (target !== undefined) {
		args.push("--target", target);
	}

	// The hook is injected before the game's own CRT is loaded. A static CRT keeps `VCRUNTIME140.dll` out of its import
	// table, so a game or a mod that ships that DLL is not shadowed by the hook.
	const env = { ...process.env };
	env.RUSTFLAGS = inheritedFlags || undefined;
	if (staticCrt) {
		env.RUSTFLAGS = `${env.RUSTFLAGS ?? ""} -C target-feature=+crt-static`.trim();
	}

	const result = spawnSync("cargo", args, { cwd: srcTauri, stdio: "inherit", env });
	if (result.status !== 0) {
		if (archName === "32-bit") {
			console.error("The 32-bit hook and injector need the Rust target i686-pc-windows-msvc.");
			console.error("Install it with: rustup target add i686-pc-windows-msvc");
		}
		process.exit(result.status ?? 1);
	}
}

// The native hook is the host architecture. On x64 (the usual Windows build) cargo already writes it to
// `target/<profile>`; on ARM64 the explicit target keeps the two builds from overwriting each other.
const nativeTarget = process.arch === "arm64" ? arm64Target : undefined;
build("gobbo-overlay-hook", "native", nativeTarget, undefined, true);
if (nativeTarget !== undefined) {
	copyFileSync(path.join(targetDir, nativeTarget, profile, "gobbo_overlay_hook.dll"), devNative);
}
build("gobbo-overlay-hook", "32-bit", x86Target, undefined, true);
build("gobbo-overlay-launch", "32-bit", x86Target, "gobbo_overlay_inject32");
build("gobbo-overlay-probe", "native", nativeTarget);
copyFileSync(path.join(targetDir, ...(nativeTarget === undefined ? [] : [nativeTarget]), profile, "overlay-probe.exe"), devProbe);
build("gobbo-overlay-probe", "32-bit", x86Target);
build("gobbo-overlay-dump", "32-bit", x86Target, "gobbo_overlay_dump32");

for (const [file, description] of [
	[devNative, "The overlay hook"],
	[x86Dll, "The 32-bit overlay hook"],
	[x86Injector, "The 32-bit overlay injector"],
	[devProbe, "The overlay probe"],
	[x86Probe, "The 32-bit overlay probe"],
	[x86Dump32, "The 32-bit overlay dumper"],
] as const) {
	if (!existsSync(file)) {
		console.error(`${description} was not built at ${file}.`);
		process.exit(1);
	}
}

mkdirSync(stagedDir, { recursive: true });
for (const [source, destinations] of [
	[devNative, [stagedNative]],
	[x86Dll, [dev32, staged32]],
	[devPdb, [stagedPdb]],
	[x86Pdb, [devPdb32, stagedPdb32]],
	[x86Injector, [devInjector, stagedInjector]],
	[devProbe, [stagedProbe]],
	[x86Probe, [devProbe32, stagedProbe32]],
	[x86Dump32, [devDump32, stagedDump32]],
] as const) {
	if (existsSync(source)) {
		for (const destination of destinations) {
			copyFileSync(source, destination);
		}
	}
}

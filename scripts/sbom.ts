import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/// A CycloneDX 1.5 component: enough for an SBOM viewer to name the package, its version and its license.
interface Component {
	type: "library";
	name: string;
	version: string;
	purl: string;
	licenses: Array<{ license: { id?: string; name?: string } } | { expression: string }>;
}

const LICENSE_EXPRESSION = /\s(OR|AND|WITH)\s/;

const root = join(import.meta.dir, "..");
const out = join(root, "target", "sbom");
const packageJson: unknown = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const version = isObject(packageJson) && typeof packageJson.version === "string" ? packageJson.version : "0.0.0";
const timestamp = new Date().toISOString();
const components = new Map<string, Component>();

function isObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}

function npmLicense(value: unknown): string | undefined {
	if (typeof value === "string") return value;

	if (isObject(value) && typeof value.type === "string") return value.type;
	return undefined;
}

function add(component: Component) {
	if (!components.has(component.purl)) components.set(component.purl, component);
}

/// `MIT OR Apache-2.0` is an SPDX expression; one identifier is a license id.
function cargoLicense(value: string | null | undefined): Component["licenses"] {
	const license = value?.trim();

	if (!license) return [{ license: { name: "unknown" } }];

	if (LICENSE_EXPRESSION.test(license)) return [{ expression: license }];
	return [{ license: { id: license } }];
}

const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
	.split("\n")
	.find((line) => line.startsWith("host: "))
	?.slice("host: ".length)
	.trim();
const metadata: unknown = JSON.parse(
	execFileSync(
		"cargo",
		[
			"metadata",
			"--manifest-path",
			"src-tauri/Cargo.toml",
			"--format-version",
			"1",
			"--locked",
			...(host ? ["--filter-platform", host] : []),
		],
		{
			encoding: "utf8",
			cwd: root,
			maxBuffer: 128 * 1024 * 1024,
		},
	),
);

if (!(isObject(metadata) && Array.isArray(metadata.packages))) throw new Error("cargo metadata has no packages");

for (const pkg of metadata.packages) {
	if (!isObject(pkg) || typeof pkg.name !== "string" || typeof pkg.version !== "string") continue;
	add({
		type: "library",
		name: pkg.name,
		version: pkg.version,
		purl: `pkg:cargo/${pkg.name}@${pkg.version}`,
		licenses: cargoLicense(typeof pkg.license === "string" ? pkg.license : null),
	});
}

/// Every installed JavaScript package, including a hoisted transitive one. The lockfile names more than the direct
/// dependencies; a package folder is the simple source of its license.
function javascriptPackages(dir: string) {
	if (!existsSync(dir)) return;

	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
		const path = join(dir, entry.name);

		if (entry.name.startsWith("@")) {
			javascriptPackages(path);
			continue;
		}

		const manifest = join(path, "package.json");

		if (!existsSync(manifest)) continue;
		const pkg: unknown = JSON.parse(readFileSync(manifest, "utf8"));

		if (!isObject(pkg) || typeof pkg.name !== "string" || typeof pkg.version !== "string") continue;
		const license = npmLicense(pkg.license);
		add({
			type: "library",
			name: pkg.name,
			version: pkg.version,
			purl: `pkg:npm/${pkg.name.replace("/", "%2F")}@${pkg.version}`,
			licenses: cargoLicense(license),
		});
	}
}

javascriptPackages(join(root, "node_modules"));

const sorted = [...components.values()].toSorted(
	(left, right) => left.name.localeCompare(right.name) || left.version.localeCompare(right.version),
);
const sbom = {
	bomFormat: "CycloneDX",
	specVersion: "1.5",
	version: 1,
	metadata: {
		timestamp,
		tools: [{ vendor: "Gobbo", name: "scripts/sbom.ts", version }],
		component: { type: "application", name: "gobbo", version },
	},
	components: sorted,
};
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "sbom.cdx.json"), `${JSON.stringify(sbom, null, "\t")}\n`);

const licenseOf = (component: Component) =>
	component.licenses
		.map((entry) => ("expression" in entry ? entry.expression : (entry.license.id ?? entry.license.name ?? "unknown")))
		.join(" OR ");
const row = (component: Component) => `| ${component.name} | ${component.version} | ${licenseOf(component)} |`;
const rust = sorted.filter((component) => component.purl.startsWith("pkg:cargo/")).map((component) => row(component));
const npm = sorted.filter((component) => component.purl.startsWith("pkg:npm/")).map((component) => row(component));
const notices = `# Third-party notices\n\nGobbo Mods Organizer is licensed under GPL-3.0-only; see \`LICENSE\`. This file lists the third-party\npackages an installed build uses and the licenses they declare. Generated ${timestamp}.\n\n## Rust crates (${rust.length})\n\n| Package | Version | License |\n| --- | --- | --- |\n${rust.join("\n")}\n\n## JavaScript packages (${npm.length})\n\n| Package | Version | License |\n| --- | --- | --- |\n${npm.join("\n")}\n`;
writeFileSync(join(out, "THIRD-PARTY-NOTICES.md"), notices);
process.stdout.write(`SBOM: ${sorted.length} components (${rust.length} Rust, ${npm.length} JavaScript) in ${out}.\n`);

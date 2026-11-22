import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.cwd();
const src = join(root, "src");

const failures: string[] = [];
const warnings: string[] = [];

function walk(dir: string): string[] {
	const files: string[] = [];

	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		const path = join(dir, entry.name);

		if (entry.isDirectory()) files.push(...walk(path));
		else if (entry.isFile() && (entry.name.endsWith(".ts") || entry.name.endsWith(".tsx"))) files.push(path);
	}

	return files;
}

function report(path: string, message: string) {
	failures.push(`${relative(root, path)}: ${message}`);
}

function warn(path: string, message: string) {
	warnings.push(`${relative(root, path)}: ${message}`);
}

/// Every legacy alias. The generated ui layer never used `border-brand`, so its check keeps the smaller subset.
const LEGACY_TOKENS = ["bg-surf", "text-ink", "border-line", "bg-brand", "text-brand", "text-danger", "bg-danger", "border-brand"] as const;
const GENERATED_UI_TOKENS = LEGACY_TOKENS.filter((token) => token !== "border-brand");

const uiDir = join(src, "components", "ui");
const files = walk(src);

for (const path of files) {
	const text = readFileSync(path, "utf8");
	const inUi = path.startsWith(uiDir);

	if (text.includes("@base-ui/")) {
		report(path, "imports @base-ui; this project uses radix-ui (see DESIGN.md)");
	}

	if (inUi) {
		for (const forbidden of ["@/i18n", "@/app/", "@/lib/", "@/views/"]) {
			if (text.includes(forbidden)) report(path, `the generated ui layer must not import ${forbidden}`);
		}

		for (const token of GENERATED_UI_TOKENS) {
			if (text.includes(token)) report(path, `the generated ui layer must not use the app token ${token}`);
		}

		continue;
	}

	if (path.endsWith(".tsx")) {
		for (const element of ["input", "select", "textarea"]) {
			if (new RegExp(`<${element}\\b`).test(text)) {
				report(path, `raw <${element}> outside src/components/ui; use the shadcn component`);
			}
		}

		const buttons = [...text.matchAll(/<button\b/g)].length;

		if (buttons > 0) warn(path, `${buttons} raw <button> element(s); use Button unless this is a documented custom control`);
	}

	for (const token of LEGACY_TOKENS) {
		if (text.includes(token)) warn(path, `legacy app token ${token}; migrate to a semantic shadcn token`);
	}
}

const indexCss = readFileSync(join(src, "index.css"), "utf8");

if (!indexCss.includes("@custom-variant dark")) {
	failures.push("src/index.css: missing @custom-variant dark; dark: variants depend on the OS theme");
}

const indexHtml = join(root, "index.html");

if (!readFileSync(indexHtml, "utf8").includes('class="dark"')) {
	failures.push('index.html: <html> needs class="dark" so the app theme, not the OS theme, selects dark variants');
}

if (warnings.length > 0) {
	console.warn(`UI warnings (${warnings.length}):`);

	for (const warning of warnings) console.warn(`  ${warning}`);
}

if (failures.length > 0) {
	console.error(`UI check failed (${failures.length}):`);

	for (const failure of failures) console.error(`  ${failure}`);
	process.exit(1);
}

console.warn("UI check passed.");

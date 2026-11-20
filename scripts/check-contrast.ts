import { readFileSync } from "node:fs";
import { join } from "node:path";

const VAR_PATTERN = /^var\(--([a-z0-9-]+)\)$/i;
const HEX_PATTERN = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;
const RGB_PATTERN = /^rgb\(\s*(\d{1,3})\s+(\d{1,3})\s+(\d{1,3})\s*\)$/i;
const OKLCH_PATTERN = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*[\d.]+%?)?\s*\)$/i;

const css = readFileSync(join(process.cwd(), "src", "index.css"), "utf8");
const declarations = new Map<string, string>();

for (const match of css.matchAll(/--([a-z0-9-]+)\s*:\s*([^;]+);/gi)) {
	const name = match[1];
	const value = match[2];

	if (name && value) declarations.set(name, value.trim());
}

function resolve(name: string, seen = new Set<string>()): string {
	if (seen.has(name)) throw new Error(`cyclic token --${name}`);
	seen.add(name);
	const value = declarations.get(name);

	if (!value) throw new Error(`missing token --${name}`);
	const variable = value.match(VAR_PATTERN);

	if (variable?.[1]) return resolve(variable[1], seen);
	return value;
}

type RGB = [number, number, number];

function srgbToLinear(channel: number): number {
	return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(channel: number): number {
	return channel <= 0.0031308 ? 12.92 * channel : 1.055 * channel ** (1 / 2.4) - 0.055;
}

function hexToRgb(hex: string): RGB {
	const match = hex.match(HEX_PATTERN);

	if (!match?.[1]) throw new Error(`not a hex colour: ${hex}`);
	const value =
		match[1].length === 3
			? match[1]
					.split("")
					.map((part) => part + part)
					.join("")
			: match[1];
	return [
		Number.parseInt(value.slice(0, 2), 16) / 255,
		Number.parseInt(value.slice(2, 4), 16) / 255,
		Number.parseInt(value.slice(4, 6), 16) / 255,
	];
}

function rgbStringToRgb(value: string): RGB {
	const match = value.match(RGB_PATTERN);
	const [, r, g, b] = match ?? [];

	if (r === undefined || g === undefined || b === undefined) {
		throw new Error(`not an rgb() colour: ${value}`);
	}

	return [Number(r) / 255, Number(g) / 255, Number(b) / 255];
}

function oklchToSrgb(value: string): RGB {
	const match = value.match(OKLCH_PATTERN);
	const [, lightness, chroma, hue] = match ?? [];

	if (lightness === undefined || chroma === undefined || hue === undefined) {
		throw new Error(`not an oklch colour: ${value}`);
	}

	const L = Number.parseFloat(lightness);
	const C = Number.parseFloat(chroma);
	const H = (Number.parseFloat(hue) * Math.PI) / 180;
	const a = C * Math.cos(H);
	const b = C * Math.sin(H);
	const l1 = L + 0.3963377774 * a + 0.2158037573 * b;
	const m1 = L - 0.1055613458 * a - 0.0638541728 * b;
	const s1 = L - 0.0894841775 * a - 1.291485548 * b;
	const l = l1 ** 3;
	const m = m1 ** 3;
	const s = s1 ** 3;
	const linear: RGB = [
		4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
		-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
		-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
	];
	return [
		Math.min(1, Math.max(0, linearToSrgb(linear[0]))),
		Math.min(1, Math.max(0, linearToSrgb(linear[1]))),
		Math.min(1, Math.max(0, linearToSrgb(linear[2]))),
	];
}

function parseColor(value: string): RGB {
	if (HEX_PATTERN.test(value)) return hexToRgb(value);

	if (RGB_PATTERN.test(value)) return rgbStringToRgb(value);
	return oklchToSrgb(value);
}

function luminance(rgb: RGB): number {
	const [r = 0, g = 0, b = 0] = rgb.map((channel) => srgbToLinear(channel));
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRgb(foreground: RGB, background: RGB): number {
	const first = luminance(foreground);
	const second = luminance(background);
	const [high, low] = first > second ? [first, second] : [second, first];
	return (high + 0.05) / (low + 0.05);
}

function blend(foreground: string, background: string, alpha: number): string {
	const fg = parseColor(resolve(foreground));
	const bg = parseColor(resolve(background));
	const mixed = fg.map((channel, index) => channel * alpha + (bg[index] ?? 0) * (1 - alpha));
	return `rgb(${Math.round((mixed[0] ?? 0) * 255)} ${Math.round((mixed[1] ?? 0) * 255)} ${Math.round((mixed[2] ?? 0) * 255)})`;
}

function toColor(value: string): RGB {
	if (value.startsWith("#") || value.startsWith("rgb") || value.startsWith("oklch")) return parseColor(value);
	return parseColor(resolve(value));
}

const checks: Array<{
	label: string;
	foreground: string;
	background: string;
	minimum: number;
}> = [
	{
		label: "foreground on background",
		foreground: "foreground",
		background: "background",
		minimum: 4.5,
	},
	{
		label: "foreground on card",
		foreground: "foreground",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "foreground on popover",
		foreground: "foreground",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "foreground on accent",
		foreground: "foreground",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "foreground on secondary",
		foreground: "foreground",
		background: "secondary",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on background",
		foreground: "muted-foreground",
		background: "background",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on card",
		foreground: "muted-foreground",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on muted",
		foreground: "muted-foreground",
		background: "muted",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on popover",
		foreground: "muted-foreground",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on accent",
		foreground: "muted-foreground",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "muted-foreground on secondary",
		foreground: "muted-foreground",
		background: "secondary",
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on card",
		foreground: "subtle-foreground",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on muted",
		foreground: "subtle-foreground",
		background: "muted",
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on popover",
		foreground: "subtle-foreground",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on accent",
		foreground: "subtle-foreground",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on secondary",
		foreground: "subtle-foreground",
		background: "secondary",
		minimum: 4.5,
	},
	{
		label: "primary on background",
		foreground: "primary",
		background: "background",
		minimum: 4.5,
	},
	{
		label: "primary on card",
		foreground: "primary",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "primary on popover",
		foreground: "primary",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "primary on accent",
		foreground: "primary",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "primary-foreground on primary",
		foreground: "primary-foreground",
		background: "primary",
		minimum: 4.5,
	},
	{
		label: "destructive on card",
		foreground: "destructive",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "destructive on popover",
		foreground: "destructive",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "destructive on accent",
		foreground: "destructive",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "destructive on the invalid input",
		foreground: "destructive",
		background: blend("input", "background", 0.3),
		minimum: 4.5,
	},
	{
		label: "draft on card",
		foreground: "draft",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "draft on popover",
		foreground: "draft",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "draft on accent",
		foreground: "draft",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "applied on card",
		foreground: "applied",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "applied on popover",
		foreground: "applied",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "applied on accent",
		foreground: "applied",
		background: "accent",
		minimum: 4.5,
	},
	{ label: "win on card", foreground: "win", background: "card", minimum: 4.5 },
	{
		label: "win on popover",
		foreground: "win",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "win on accent",
		foreground: "win",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "lose on card",
		foreground: "lose",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "lose on popover",
		foreground: "lose",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "lose on accent",
		foreground: "lose",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "update on card",
		foreground: "update",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "update on popover",
		foreground: "update",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "update on accent",
		foreground: "update",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "idle on card",
		foreground: "idle",
		background: "card",
		minimum: 4.5,
	},
	{
		label: "idle on popover",
		foreground: "idle",
		background: "popover",
		minimum: 4.5,
	},
	{
		label: "idle on accent",
		foreground: "idle",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "input border on background",
		foreground: "input",
		background: "background",
		minimum: 3,
	},
	{
		label: "input border on card",
		foreground: "input",
		background: "card",
		minimum: 3,
	},
	{
		label: "input border on popover",
		foreground: "input",
		background: "popover",
		minimum: 3,
	},
	{
		label: "focus ring at 50% on card",
		foreground: blend("ring", "card", 0.5),
		background: "card",
		minimum: 3,
	},
	{
		label: "focus ring at 50% on muted",
		foreground: blend("ring", "muted", 0.5),
		background: "muted",
		minimum: 3,
	},
	{
		label: "focus ring at 50% on accent",
		foreground: blend("ring", "accent", 0.5),
		background: "accent",
		minimum: 3,
	},
	{
		label: "focus ring at 50% on popover",
		foreground: blend("ring", "popover", 0.5),
		background: "popover",
		minimum: 3,
	},
	{
		label: "accent foreground on accent",
		foreground: "accent-foreground",
		background: "accent",
		minimum: 4.5,
	},
	{
		label: "destructive on its 10% tint",
		foreground: "destructive",
		background: blend("destructive", "card", 0.1),
		minimum: 4.5,
	},
	{
		label: "destructive on its 20% tint",
		foreground: "destructive",
		background: blend("destructive", "card", 0.2),
		minimum: 4.5,
	},
	{
		label: "foreground on the input surface",
		foreground: "foreground",
		background: blend("input", "card", 0.3),
		minimum: 4.5,
	},
	{
		label: "muted-foreground on the input surface",
		foreground: "muted-foreground",
		background: blend("input", "card", 0.3),
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on the input surface",
		foreground: "subtle-foreground",
		background: blend("input", "card", 0.3),
		minimum: 4.5,
	},
	{
		label: "foreground on the muted surface",
		foreground: "foreground",
		background: blend("muted", "card", 0.25),
		minimum: 4.5,
	},
	{
		label: "muted-foreground on the muted surface",
		foreground: "muted-foreground",
		background: blend("muted", "card", 0.25),
		minimum: 4.5,
	},
	{
		label: "subtle-foreground on the muted surface",
		foreground: "subtle-foreground",
		background: blend("muted", "card", 0.25),
		minimum: 4.5,
	},
];

let failed = 0;

for (const check of checks) {
	const foreground = toColor(check.foreground);
	const background = toColor(check.background);
	const ratio = contrastRgb(foreground, background);
	const passed = ratio >= check.minimum;

	if (!passed) failed += 1;

	if (!passed) console.error(`FAIL ${check.label}: ${ratio.toFixed(2)}:1 (minimum ${check.minimum}:1)`);
}

if (failed > 0) {
	console.error(`Contrast check failed: ${failed} pair(s) below the WCAG threshold.`);
	process.exit(1);
}

console.warn(`Contrast check passed (${checks.length} pairs).`);

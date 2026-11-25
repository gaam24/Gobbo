const TRAILING_SLASHES = /\/+$/;

/// Text from a path field as a path. The spaces around it go, and so does one pair of quotes: Windows' "Copy as path"
/// puts the path in quotes, and the user should not have to remove them by hand.
export function cleanPath(text: string): string {
	const trimmed = text.trim();
	const quoted = trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"');
	return quoted ? trimmed.slice(1, -1).trim() : trimmed;
}

/// The path of `file` relative to `folder`, with `/` separators, or `null` when the file is not inside the folder. Both
/// `\` and `/` separate the parts of either path. The folder comparison ignores case, as Windows does.
export function relativeToFolder(folder: string, file: string): string | null {
	const root = withSlashes(cleanPath(folder)).replace(TRAILING_SLASHES, "");
	const path = withSlashes(file);

	if (root === "" || !path.toLowerCase().startsWith(`${root.toLowerCase()}/`)) return null;

	const relative = path.slice(root.length + 1);
	return relative === "" ? null : relative;
}

function withSlashes(text: string): string {
	return text.replaceAll("\\", "/");
}

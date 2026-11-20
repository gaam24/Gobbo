/// Archive formats the import reads. File pickers, drag and drop and the import all use this list.
export const ARCHIVE_EXTENSIONS = ["zip", "7z", "rar"];

export function isArchive(path: string): boolean {
	const lower = path.toLowerCase();
	return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(`.${extension}`));
}

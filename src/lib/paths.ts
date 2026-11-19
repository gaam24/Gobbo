/// Splits a path on both separators, so a file picked or dropped on either system shows the same name.
const PATH_SPLIT = /[\\/]/;

/// The last part of a path: the file name without its folders.
export function fileName(path: string): string {
	return path.split(PATH_SPLIT).pop() ?? path;
}

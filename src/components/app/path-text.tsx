import { cn } from "cn";

/// A file or folder path on one line. A path that does not fit is shortened at the start, so the end that names the
/// file or the game folder stays visible; the whole path is shown on hover. The `bdi` keeps the slashes in reading
/// order inside the right-to-left box that moves the ellipsis to the start.
export function PathText({ path, className }: { path: string; className?: string }) {
	return (
		<span className={cn("block min-w-0 truncate text-left font-mono text-xs [direction:rtl]", className)} title={path}>
			<bdi>{path}</bdi>
		</span>
	);
}

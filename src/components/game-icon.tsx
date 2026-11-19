import { cn } from "cn";
import { commands } from "@/bindings";
import { unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// Icon cache at module level: one IPC request per game for the whole time the app runs.
const cache = new Map<string, Promise<string | null>>();

function iconOf(id: string): Promise<string | null> {
	let icon = cache.get(id);
	if (!icon) {
		icon = (async () => {
			try {
				return await unwrap(commands.gameIcon(id));
			} catch {
				// a missing icon file is not an error worth showing; the initial stands in
				return null;
			}
		})();
		cache.set(id, icon);
	}
	return icon;
}

/// Game icon from `games/<id>/icon.webp` (or `icon.png`), or without the file, a circle with the initial of the name.
/// The caller sets size and rounding through `className`, e.g. `size-4 rounded-sm`.
export function GameIcon({ id, name, className }: { id: string; name: string; className?: string }) {
	const icon = useLoad(id, () => iconOf(id));

	if (icon) {
		return <img src={icon} alt="" aria-hidden className={cn("size-4 shrink-0 rounded-sm object-cover", className)} />;
	}
	return (
		<span
			aria-hidden
			className={cn("bg-muted text-muted-foreground grid size-4 shrink-0 place-items-center rounded-sm font-semibold", className)}
		>
			{name.slice(0, 1).toUpperCase()}
		</span>
	);
}

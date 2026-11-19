import { QuickCheck } from "@/views/games/editor/quick-check";
import { QuickGame } from "@/views/games/editor/quick-game";
import { QuickLaunch } from "@/views/games/editor/quick-launch";
import { QuickMods } from "@/views/games/editor/quick-mods";
import type { QuickProps } from "@/views/games/editor/quick-types";

/// The quick part of the editor: the folder, the mod layout, the program and the check. Each card receives the same
/// values; a card only reads what it draws.
export function QuickSetup(props: QuickProps) {
	return (
		<div className="grid gap-8">
			<QuickGame {...props} />
			<QuickMods {...props} />
			<QuickLaunch {...props} />
			<QuickCheck {...props} />
		</div>
	);
}

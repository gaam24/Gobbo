import { Suspense } from "react";

import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useGlobalShortcuts } from "@/app/hooks/use-global-shortcuts";
import { CommandPalette } from "@/app/lazy-views";
import { useOpenPalette } from "@/app/palette/use-open-palette";

/// The command palette and the global shortcuts that open it. It subscribes to the workspace because the palette lists
/// mods and profiles; the shell itself stays free of that dependency.
export function CommandPaletteHost() {
	const { palette } = useUiState();
	const { closePalette } = useUiActions();
	const openPalette = useOpenPalette();
	useGlobalShortcuts({ openPalette });

	if (!palette) return null;

	return (
		<Suspense fallback={null}>
			<CommandPalette open onOpenChange={(open) => !open && closePalette()} items={palette} />
		</Suspense>
	);
}

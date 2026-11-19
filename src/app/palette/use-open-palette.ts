import { useCallback, useEffect, useRef } from "react";

import { useUiActions } from "@/app/contexts/ui-context";
import { usePaletteItems } from "@/app/palette/use-palette-items";

/// Opens the palette with a snapshot of the current state. The snapshot is deliberate: a background status refresh must
/// not move an item under the user's keyboard while the palette is open. The ref keeps the callback stable for effects
/// while still reading the latest snapshot when it is called.
export function useOpenPalette(): () => void {
	const build = usePaletteItems();
	const { openPalette } = useUiActions();
	const latest = useRef(build);
	useEffect(() => {
		latest.current = build;
	});

	return useCallback(() => openPalette(latest.current()), [openPalette]);
}

import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";

/// What the game editor is open on: a new definition, or an existing one by id. The Games page renders the editor;
/// a settings page can open it without knowing where it lives.
export interface GameEditorTarget {
	/// `null` for a new definition.
	id: string | null;
}

interface GameEditor {
	target: GameEditorTarget | null;
	openGameEditor: (id?: string) => void;
	closeGameEditor: () => void;
}

const GameEditorContext = createContext<GameEditor | null>(null);

export function useGameEditor(): GameEditor {
	const context = useContext(GameEditorContext);

	if (!context) {
		throw new Error("useGameEditor must be used inside <GameEditorProvider>.");
	}

	return context;
}

/// The editor is one screen with one draft, so its target is app-wide state rather than a prop threaded through the
/// Games page and the game settings.
export function GameEditorProvider({ children }: { children: ReactNode }) {
	const [target, setTarget] = useState<GameEditorTarget | null>(null);
	const openGameEditor = useCallback((id?: string) => setTarget({ id: id ?? null }), []);
	const closeGameEditor = useCallback(() => setTarget(null), []);

	const value = useMemo((): GameEditor => ({ target, openGameEditor, closeGameEditor }), [closeGameEditor, openGameEditor, target]);

	return <GameEditorContext.Provider value={value}>{children}</GameEditorContext.Provider>;
}

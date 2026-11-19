import type { DefinitionCheck_Serialize, FolderInspection_Serialize, LoaderPresetView_Serialize } from "@/bindings";
import type { DraftIssue, GameDraft } from "@/views/games/editor/draft";

/// The quick cards the user can choose. `custom` means the definition has more than one kind of place and the
/// advanced form is the only honest summary.
export type LayoutMode = "loader" | "slot" | "mirror" | "custom";

/// The values the four quick cards share. Passing one object keeps the cards presentational and avoids threading a
/// dozen positional props through each one.
export interface QuickProps {
	draft: GameDraft;
	onChange: (draft: GameDraft) => void;
	folder: string;
	onFolderChange: (folder: string) => void;
	onBrowse: () => void;
	inspection: FolderInspection_Serialize | null;
	inspecting: boolean;
	inspectError: string | null;
	presets: LoaderPresetView_Serialize[];
	layout: LayoutMode;
	onLayoutChange: (layout: LayoutMode, presetId?: string) => void;
	check: DefinitionCheck_Serialize | null;
	issues: DraftIssue[];
	editing: boolean;
	idWarning: string | null;
}

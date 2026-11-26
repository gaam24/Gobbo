import { Download, Layers, OctagonX, Play, RefreshCw, RotateCcw, ToggleRight, Upload, Wrench } from "lucide-react";
import type { GameAction, GameTone } from "@/app/selectors/game-presentation";
import { StatusBadge } from "@/components/app/status-badge";

/// The icon of an action. The same action keeps the same icon in the primary button and in the menu.
export function actionIcon(action: GameAction) {
	switch (action.id) {
		case "play":
		case "apply-and-play":
			return <Play />;
		case "apply":
			return <Layers />;
		case "discard":
			return <RotateCcw />;
		case "enable":
			return <ToggleRight />;
		case "retry":
			return <RefreshCw />;
		case "stop":
			return <OctagonX />;
		case "import":
			return <Upload />;
		case "fix-setup":
		case "fix-problems":
			return <Wrench />;
		case "install-loader":
			return <Download />;
		default:
			return <Play />;
	}
}

/// The state chip uses the shared status tones. The text says the state, so colour is not the only signal.
export function StateChip({ tone, children }: { tone: GameTone; children: string }) {
	return (
		<StatusBadge tone={tone} dot className="h-8 max-w-full min-w-0 shrink px-3 text-sm">
			<span className="truncate">{children}</span>
		</StatusBadge>
	);
}

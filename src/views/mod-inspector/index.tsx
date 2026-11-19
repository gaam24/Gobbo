import type { CSSProperties } from "react";
import type { GameRuntime } from "@/app/selectors/game-presentation";
import type { Status, UpdateReport } from "@/bindings";
import { PanelHandle } from "@/components/panel-handle";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { useTranslation } from "@/i18n";
import { checkedAtOf } from "@/lib/mods";
import { BatchPanel } from "./batch-panel";
import { DeploymentSummary, type GettingStartedActions } from "./deployment-summary";
import { type ModActions, ModDetails } from "./mod-details";

const MIN_WIDTH = 280;
const MAX_WIDTH = 900;
/// Room the mod list keeps next to the panel.
const LIST_ROOM = 600;

/// The width until the user drags it: a little more room on a big screen.
const AUTO_WIDTH = "clamp(340px, 22vw, 480px)";

/// The width as a CSS variable. React's style type has no custom properties.
function widthVariable(width: number | null): CSSProperties {
	const style: CSSProperties = {};
	return Object.assign(style, { "--inspector-width": width === null ? AUTO_WIDTH : `${width}px` });
}

/// Inspector on the right. No selection: the deployment. One mod: its details. Several: the batch panel.
export function ModInspector({
	gameId,
	gameName,
	gameRunning,
	status,
	selection,
	updates,
	busy,
	onOpenOverwrite,
	onRunTool,
	onImport,
	onDeploy,
	onOpenDownloads,
	mode,
	open,
	onOpenChange,
	width,
	onResize,
	onResetWidth,
	...actions
}: ModActions &
	GettingStartedActions & {
		gameId: string;
		gameName: string;
		/// The game is running, so an apply or a tool waits.
		gameRunning: boolean;
		status: Status;
		/// Selected folders that are in the profile. One folder shows its details, several show the batch panel.
		selection: string[];
		updates: Record<string, UpdateReport>;
		/// The long action running now, so the deployment card cannot start a second apply.
		busy: GameRuntime["busy"];
		/// `side`: a column next to the list, on a wide window. `sheet`: a panel over the list, on a narrow one, open while
		/// `open` says so.
		mode: "side" | "sheet";
		open: boolean;
		onOpenChange: (open: boolean) => void;
		/// The column's width, in pixels; `null` grows with the window until the user drags it.
		width: number | null;
		onResize: (width: number) => void;
		onResetWidth: () => void;
		onOpenOverwrite?: () => void;
		onRunTool?: (id: string) => void | Promise<void>;
	}) {
	const { t } = useTranslation();
	const mod = selection.length === 1 ? status.mods.find((item) => item.id === selection[0]) : undefined;
	const max = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, window.innerWidth - LIST_ROOM));

	const body = (
		<>
			{selection.length === 0 && (
				<DeploymentSummary
					gameName={gameName}
					status={status}
					updates={updates}
					gameRunning={gameRunning}
					busy={busy}
					onOpenOverwrite={onOpenOverwrite}
					onRunTool={onRunTool}
					onImport={onImport}
					onDeploy={onDeploy}
					onOpenDownloads={onOpenDownloads}
				/>
			)}
			{selection.length > 1 && (
				<BatchPanel status={status} selection={selection} onSetEnabled={actions.onSetEnabled} onSelect={actions.onSelect} />
			)}
			{mod && (
				<ModDetails
					key={mod.id}
					gameId={gameId}
					status={status}
					mod={mod}
					update={updates[mod.id]}
					checkedAt={checkedAtOf(mod)}
					{...actions}
				/>
			)}
		</>
	);

	if (mode === "sheet") {
		return (
			<Sheet open={open} onOpenChange={onOpenChange}>
				<SheetContent closeLabel={t("common.close")} className="w-[min(30rem,92vw)] gap-0 p-0 sm:max-w-[30rem]">
					<SheetHeader className="h-12 shrink-0 flex-row items-center gap-2 border-b px-5 py-0 pr-12">
						<SheetTitle className="min-w-0 flex-1 truncate text-sm">
							{mod?.info.name ?? (selection.length > 1 ? t("mods.selected", { count: selection.length }) : gameName)}
						</SheetTitle>
					</SheetHeader>
					<div className="min-h-0 flex-1 overflow-y-auto">{body}</div>
				</SheetContent>
			</Sheet>
		);
	}

	return (
		<>
			<PanelHandle
				label={t("header.resizeInspector")}
				hint={t("header.resizeInspectorHint")}
				width={width === null ? null : Math.min(width, max)}
				min={MIN_WIDTH}
				max={max}
				onResize={onResize}
				onReset={onResetWidth}
			/>
			<aside
				aria-label={gameName}
				style={widthVariable(width)}
				className="bg-card flex w-(--inspector-width) max-w-[calc(100vw-37.5rem)] min-w-70 shrink-0 flex-col overflow-y-auto border-l"
			>
				{body}
			</aside>
		</>
	);
}

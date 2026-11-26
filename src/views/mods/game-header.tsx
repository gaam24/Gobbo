import {
	ArchiveRestore,
	ChevronDown,
	CircleAlert,
	Download,
	Files,
	Layers,
	ListOrdered,
	OctagonX,
	PanelRightClose,
	PanelRightOpen,
	Play,
	RefreshCw,
	RotateCcw,
	SlidersHorizontal,
	Stethoscope,
	ToggleRight,
	Undo2,
	Upload,
	Wrench,
} from "lucide-react";
import { useState } from "react";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { type GameAction, type GameTone, gamePresentation } from "@/app/selectors/game-presentation";
import type { Status } from "@/bindings";
import { PageHeader } from "@/components/app/page-layout";
import { StatusBadge } from "@/components/app/status-badge";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { CollectionBadge } from "@/views/mods/collection-badge";
import { ProfileActions, ProfileSwitcher } from "@/views/profile-tabs";

/// The game, its profiles and one state. The one primary action starts the game (applying the list first when it
/// has changes). Everything else, such as discarding changes, undoing an apply or closing the game, is in the menu.
export function GameHeader({ gameId, status }: { gameId: string; status: Status }) {
	const {
		gameName,
		gameRunning,
		busy,
		gameActions,
		deploy: onDeploy,
		launch: onLaunch,
		pickPack,
		pickPackLink,
		pickArchives,
		toMods,
	} = useWorkspace();
	const { inspectorOpen } = useUiState();
	const { toggleInspector, openDialog } = useUiActions();
	const { openGameSettings } = useNavigation();
	const { discardChanges, undo, stopGame, switchProfile, createProfile, copyProfileData, renameProfile, deleteProfile } = gameActions;
	const inspectorCollapsed = !inspectorOpen;
	const { t } = useTranslation();
	const [forceCloseOpen, setForceCloseOpen] = useState(false);
	const [discardOpen, setDiscardOpen] = useState(false);
	/// One presentation of the state. The chip, the primary action and the menu all read it, so they cannot drift apart.
	const presentation = gamePresentation(status, { gameRunning, busy });
	const defaultExecutable = status.executables.find((exe) => exe.default)?.name ?? null;
	const applyDraft = presentation.canApply;
	const canUndo = presentation.canUndo;

	const runAction = (action: GameAction) => {
		switch (action.id) {
			case "play":
				void onLaunch(defaultExecutable, applyDraft);
				return;
			case "apply-and-play":
				void onLaunch(defaultExecutable, true);
				return;
			case "apply":
				onDeploy();
				return;
			case "discard":
				setDiscardOpen(true);
				return;
			case "stop":
				setForceCloseOpen(true);
				return;
			case "fix-setup":
				openDialog({ kind: "setup" });
				return;
			case "install-loader":
				void gameActions.installLoader(gameId);
				return;
			case "fix-problems": {
				// The setup checklist repeats the problems, but the action that fixes them is the list itself.
				const problems = document.getElementById("mods-problems");
				problems?.scrollIntoView({ block: "nearest" });
				problems?.focus();
				return;
			}
			case "import":
				void pickArchives();
				return;
			case "enable":
				toMods();
				return;
			case "retry":
				return;
			default:
				return;
		}
	};

	/// The chip is only for the one state the primary action cannot explain on its own: changes waiting to be applied.
	/// Every other state (applied, running, busy) is already said by the primary action, so the header stays quiet.
	const chip = () => {
		if (presentation.stateKey !== "header.draftChanges") return null;
		const label = t(presentation.stateKey, presentation.stateParams);
		const hint = presentation.stateHintKey ? t(presentation.stateHintKey, presentation.stateParams) : undefined;
		return (
			<Button
				variant="ghost"
				title={hint}
				className="h-auto rounded-full p-0 hover:bg-transparent hover:brightness-125"
				disabled={gameRunning}
				onClick={onDeploy}
			>
				<StateChip tone="warning">{label}</StateChip>
			</Button>
		);
	};

	/// One primary action: the state decides whether it is Play, Apply and play, Fix or Stop.
	const primary = () => {
		const label = t(presentation.primary.labelKey, presentation.primary.params);
		/// The selector already replaces the primary action with Stop while the game runs; Stop itself stays enabled.
		const blocked = presentation.primary.disabled ?? false;

		if (blocked && presentation.primary.reasonKey && !gameRunning) {
			return (
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="inline-flex">
							<Button variant={presentation.primary.id === "stop" ? "destructive" : "default"} disabled>
								{actionIcon(presentation.primary)} {label}
							</Button>
						</span>
					</TooltipTrigger>
					<TooltipContent className="max-w-72">{t(presentation.primary.reasonKey)}</TooltipContent>
				</Tooltip>
			);
		}

		return (
			<Button
				variant={presentation.primary.id === "stop" ? "destructive" : "default"}
				disabled={blocked}
				onClick={() => runAction(presentation.primary)}
			>
				{actionIcon(presentation.primary)} {label}
			</Button>
		);
	};

	const actions = (
		<>
			{status.problems.length > 0 && (
				<Button
					variant="outline"
					size="sm"
					className="text-destructive"
					aria-label={t("health.header", { count: status.problems.length })}
					title={t("health.header", { count: status.problems.length })}
					onClick={() => openDialog({ kind: "health" })}
				>
					<CircleAlert /> <span className="tabular-nums">{status.problems.length}</span>
				</Button>
			)}
			{chip()}
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button variant="ghost" aria-label={t("header.menu.tools")} title={t("header.menu.tools")}>
						<Wrench /> <span className="hidden @5xl:inline">{t("header.menu.tools")}</span>
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" className="w-64">
					<DropdownMenuItem onSelect={() => openDialog({ kind: "conflicts" })}>
						<Files /> {t("conflicts.title")}
						{status.conflict_count > 0 && (
							<span className="text-muted-foreground ml-auto text-xs tabular-nums">{status.conflict_count}</span>
						)}
					</DropdownMenuItem>
					{status.load_order && (
						<DropdownMenuItem onSelect={() => openDialog({ kind: "load-order" })}>
							<ListOrdered /> {t("loadOrder.title")}
						</DropdownMenuItem>
					)}
					<DropdownMenuItem onSelect={() => openDialog({ kind: "snapshots" })}>
						<ArchiveRestore /> {t("snapshots.title")}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem onSelect={() => openDialog({ kind: "health" })}>
						<Stethoscope /> {t("health.title")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={() => openDialog({ kind: "setup" })}>
						<Wrench /> {t("setupCheck.title")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
			<div className="flex items-center">
				{primary()}
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant={presentation.primary.id === "stop" ? "destructive" : "default"}
							size="icon"
							aria-label={t("header.executableMenu")}
							title={t("header.executableMenu")}
						>
							<ChevronDown />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end" className="w-64">
						{status.executables.length > 0 && (
							<>
								<DropdownMenuLabel>{t("header.menu.run")}</DropdownMenuLabel>
								{status.executables.map((exe) => (
									<DropdownMenuItem
										key={exe.name}
										disabled={gameRunning || !presentation.canPlay}
										onClick={() => void onLaunch(exe.name, applyDraft)}
									>
										<Play /> {exe.name}
										{exe.default && <span className="text-subtle-foreground ml-auto text-xs">{t("header.defaultTag")}</span>}
									</DropdownMenuItem>
								))}
							</>
						)}
						{presentation.menu.length > 0 && (
							<>
								{status.executables.length > 0 && <DropdownMenuSeparator />}
								<DropdownMenuLabel>{t("header.menu.actions")}</DropdownMenuLabel>
								{presentation.menu.map((action) => (
									<DropdownMenuItem
										key={action.id}
										disabled={gameRunning || action.disabled}
										onSelect={() => {
											if (action.id === "discard") setDiscardOpen(true);
											else runAction(action);
										}}
									>
										{actionIcon(action)} {t(action.labelKey, action.params)}
									</DropdownMenuItem>
								))}
							</>
						)}
						{canUndo && !gameRunning && (
							<>
								{(status.executables.length > 0 || presentation.menu.length > 0) && <DropdownMenuSeparator />}
								<DropdownMenuLabel>{t("header.menu.history")}</DropdownMenuLabel>
								<DropdownMenuItem onSelect={() => void undo()}>
									<Undo2 /> {t("deploy.undo")}
								</DropdownMenuItem>
							</>
						)}
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
			<Button variant="ghost" size="icon" aria-label={t("mods.gameSettings")} title={t("mods.gameSettings")} onClick={openGameSettings}>
				<SlidersHorizontal />
			</Button>
			<Button
				variant="ghost"
				size="icon"
				aria-label={t(inspectorCollapsed ? "header.showInspector" : "header.hideInspector")}
				title={t(inspectorCollapsed ? "header.showInspector" : "header.hideInspector")}
				aria-expanded={!inspectorCollapsed}
				onClick={() => void toggleInspector()}
			>
				{inspectorCollapsed ? <PanelRightOpen /> : <PanelRightClose />}
			</Button>
		</>
	);

	return (
		<>
			<PageHeader
				title={<span title={status.game_subtitle ? `${gameName} — ${status.game_subtitle}` : gameName}>{gameName}</span>}
				actions={actions}
			>
				<ProfileSwitcher
					profiles={status.profiles}
					onSwitch={switchProfile}
					onCreate={createProfile}
					busy={busy !== null}
					gameRunning={gameRunning}
				/>
				<ProfileActions
					gameId={gameId}
					profiles={status.profiles}
					onCreate={createProfile}
					onCopyData={copyProfileData}
					onRename={renameProfile}
					onDelete={deleteProfile}
					onExportPack={() => openDialog({ kind: "pack-export" })}
					onImportPack={pickPack}
					onImportPackLink={pickPackLink}
				/>
				<CollectionBadge profile={status.profiles.find((profile) => profile.active)} />
			</PageHeader>
			<ConfirmDialog
				open={forceCloseOpen}
				onOpenChange={setForceCloseOpen}
				title={t("header.forceCloseConfirm.title")}
				description={t("header.forceCloseConfirm.body")}
				confirmLabel={t("header.forceClose")}
				onConfirm={stopGame}
			/>
			<ConfirmDialog
				open={discardOpen}
				onOpenChange={setDiscardOpen}
				title={t("header.discardConfirm.title", {
					count: status.draft_changes,
				})}
				description={t("header.discardConfirm.body")}
				confirmLabel={t("header.discard")}
				onConfirm={discardChanges}
			/>
		</>
	);
}

/// The icon of an action. The same action keeps the same icon in the primary button and in the menu.
function actionIcon(action: GameAction) {
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
function StateChip({ tone, children }: { tone: GameTone; children: string }) {
	return (
		<StatusBadge tone={tone} dot className="h-8 max-w-full min-w-0 shrink px-3 text-sm">
			<span className="truncate">{children}</span>
		</StatusBadge>
	);
}

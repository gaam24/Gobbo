import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useGames } from "@/app/contexts/games-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useUnsavedChanges } from "@/app/contexts/unsaved-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { DefinitionCheck_Serialize, FolderInspection_Serialize, GameOrigin, LoaderPresetView_Serialize } from "@/bindings";
import { commands } from "@/bindings";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { pickFolder, pickSaveFile } from "@/lib/tauri";
import {
	type DraftIssue,
	type GameDraft,
	inferLayout,
	localIssues,
	normalizeDraft,
	parseDraft,
	serializeDraft,
	setLoader,
} from "@/views/games/editor/draft";
import type { LayoutMode } from "@/views/games/editor/quick-types";
import { applySuggestions } from "@/views/games/editor/suggestions";

/// Everything the editor screen draws and every action it runs. The view stays a view; the draft, the checks and the
/// one save live here.
export interface GameEditorModel {
	editing: boolean;
	loaded: boolean;
	loadError: string | null;
	draft: GameDraft;
	folder: string;
	setFolder: (folder: string) => void;
	inspection: FolderInspection_Serialize | null;
	inspecting: boolean;
	inspectError: string | null;
	presets: LoaderPresetView_Serialize[];
	presetError: string | null;
	check: DefinitionCheck_Serialize | null;
	checkError: string | null;
	layout: LayoutMode;
	issues: DraftIssue[];
	idWarning: string | null;
	blocked: boolean;
	saving: boolean;
	source: string;
	sourceDirty: boolean;
	formText: string;
	sourceError: string | null;
	activeText: string;
	dirty: boolean;
	/// Where the definition came from; `null` for a new draft.
	origin: GameOrigin | null;
	/// `<data>/definitions`, the folder that holds a user's games and loaders. `null` until the data folder is read.
	definitionsFolder: string | null;
	/// Whether at least one user definition is loaded, so the folder exists and can be opened.
	hasUserDefinitions: boolean;
	openDefinitionsFolder: () => void;
	update: (draft: GameDraft) => void;
	chooseLayout: (layout: LayoutMode, presetId?: string) => void;
	browse: () => Promise<void>;
	changeSource: (source: string) => void;
	applySource: () => Promise<void>;
	save: () => Promise<void>;
	exportDefinition: () => Promise<void>;
	requestLeave: (action: () => void) => void;
	onClose: () => void;
}

export function useGameEditor(id: string | null, onClose: () => void): GameEditorModel {
	const { t } = useTranslation();
	const { games, managed, reloadGames } = useGames();
	const { gameActions, openFolderPath } = useWorkspace();
	const { attempt, notify } = useNoticeActions();
	const { setDirty: setShellDirty, requestLeave } = useUnsavedChanges();
	const editing = id !== null;

	const [draft, setDraft] = useState<GameDraft>(() => normalizeDraft({}));
	const [loaded, setLoaded] = useState(!editing);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [folder, setFolder] = useState("");
	const [inspection, setInspection] = useState<FolderInspection_Serialize | null>(null);
	const [inspecting, setInspecting] = useState(false);
	const [inspectError, setInspectError] = useState<string | null>(null);
	const [presets, setPresets] = useState<LoaderPresetView_Serialize[]>([]);
	const [presetError, setPresetError] = useState<string | null>(null);
	const [check, setCheck] = useState<DefinitionCheck_Serialize | null>(null);
	const [checkError, setCheckError] = useState<string | null>(null);
	const [layout, setLayout] = useState<LayoutMode>("slot");
	const [initialText, setInitialText] = useState<string | null>(null);
	const [origin, setOrigin] = useState<GameOrigin | null>(null);
	const [source, setSource] = useState("");
	const [sourceDirty, setSourceDirty] = useState(false);
	const [sourceError, setSourceError] = useState<string | null>(null);
	const [dirty, setDirty] = useState(false);
	const [saving, setSaving] = useState(false);
	const [dataFolder, setDataFolder] = useState<string | null>(null);

	const draftRef = useRef(draft);
	useEffect(() => {
		draftRef.current = draft;
	}, [draft]);
	const managedRef = useRef(managed);
	useEffect(() => {
		managedRef.current = managed;
	}, [managed]);
	const inspected = useRef<string | null>(null);

	// The editor is one draft; the shell asks before a screen change would lose it.
	useEffect(() => {
		setShellDirty(dirty);
		return () => setShellDirty(false);
	}, [dirty, setShellDirty]);

	// The definitions folder is shown and opened from the header, so a user can find the file the editor writes.
	useEffect(() => {
		void (async () => {
			try {
				setDataFolder(await commands.dataFolder());
			} catch {
				setDataFolder(null);
			}
		})();
	}, []);

	useEffect(() => {
		const live = { current: true };
		void (async () => {
			try {
				const list = await unwrap(commands.loaderPresets());
				if (!live.current) return;
				setPresets(list);
			} catch (failure) {
				if (live.current) setPresetError(messageOf(failure));
			}
			if (!id) return;
			try {
				const definition = await unwrap(commands.definitionSource(id));
				if (!live.current) return;
				const parsed = parseDraft(definition.text);
				setDraft(parsed);
				setInitialText(definition.text);
				setOrigin(definition.origin);
				setLayout(inferLayout(parsed));
				const gameRoot = managedRef.current?.find((game) => game.id === id)?.game_root;
				if (gameRoot) setFolder(gameRoot);
				setLoaded(true);
			} catch (failure) {
				if (!live.current) return;
				setLoadError(messageOf(failure));
				setLoaded(true);
			}
		})();
		return () => {
			live.current = false;
		};
	}, [id]);

	const inspect = useCallback(async (path: string) => {
		const trimmed = path.trim();
		if (!trimmed) return;
		setInspecting(true);
		setInspectError(null);
		try {
			const result = await unwrap(commands.inspectGameFolder(trimmed));
			setInspection(result);
			const current = draftRef.current;
			const next = applySuggestions(current, result);
			if (JSON.stringify(next) !== JSON.stringify(current)) {
				setDraft(next);
				setDirty(true);
			}
			if (!current.loader && (current.deploy.slots?.length ?? 0) === 0 && (current.deploy.mirror?.length ?? 0) === 0) {
				setLayout(result.loaders.length > 0 ? "loader" : "slot");
			}
		} catch (failure) {
			setInspectError(messageOf(failure));
		}
		setInspecting(false);
	}, []);

	// A folder is read after a short pause, so typing a path does not run a walk per key.
	useEffect(() => {
		if (!loaded) return;
		const trimmed = folder.trim();
		if (!trimmed || inspected.current === trimmed) return;
		const timer = window.setTimeout(() => {
			inspected.current = trimmed;
			void inspect(trimmed);
		}, 500);
		return () => window.clearTimeout(timer);
	}, [folder, inspect, loaded]);

	const formText = useMemo(() => {
		if (!dirty && initialText !== null) return initialText;
		try {
			return serializeDraft(draft);
		} catch {
			return "";
		}
	}, [dirty, draft, initialText]);
	const activeText = sourceDirty ? source : formText;

	useEffect(() => {
		if (!loaded) return;
		const live = { current: true };
		const timer = window.setTimeout(() => {
			void (async () => {
				try {
					const result = await unwrap(commands.validateDefinition(activeText));
					if (!live.current) return;
					setCheck(result);
					setCheckError(null);
				} catch (failure) {
					if (live.current) setCheckError(messageOf(failure));
				}
			})();
		}, 250);
		return () => {
			live.current = false;
			window.clearTimeout(timer);
		};
	}, [activeText, loaded]);

	const update = (next: GameDraft) => {
		setDraft(next);
		setDirty(true);
		setSourceDirty(false);
		setSourceError(null);
	};

	const chooseLayout = (next: LayoutMode, presetId?: string) => {
		setLayout(next);
		if (next === "custom") return;
		setDirty(true);
		setSourceDirty(false);
		if (next === "loader") {
			const preset = presets.find((entry) => entry.id === (presetId ?? "")) ?? presets.find((entry) => entry.problems.length === 0);
			if (preset) {
				setDraft(setLoader({ ...draft, deploy: { ...draft.deploy, slots: [], mirror: [] } }, preset.id, ""));
			}
			return;
		}
		if (next === "slot") {
			setDraft({
				...draft,
				loader: undefined,
				deploy: {
					...draft.deploy,
					slots: draft.deploy.slots?.length ? draft.deploy.slots : [{ dir: "" }],
					mirror: [],
				},
			});
			return;
		}
		setDraft({
			...draft,
			loader: undefined,
			deploy: {
				...draft.deploy,
				slots: [],
				mirror: draft.deploy.mirror?.length ? draft.deploy.mirror : [{ dir: "" }],
			},
		});
	};

	const changeFolder = (value: string) => {
		setFolder(value);
		setInspection(null);
		setInspectError(null);
	};

	const browse = async () => {
		const picked = await pickFolder();
		if (picked) {
			inspected.current = null;
			changeFolder(picked);
		}
	};

	const changeSource = (value: string) => {
		setSource(value);
		setSourceDirty(true);
		setSourceError(null);
	};

	const applySource = async () => {
		try {
			const parsed = parseDraft(source);
			const result = await unwrap(commands.validateDefinition(source));
			if (!result.ok) {
				setSourceError(result.problems.map((problem) => problem.message).join("\n"));
				return;
			}
			setDraft(parsed);
			setSourceDirty(false);
			setSourceError(null);
			setDirty(true);
		} catch (failure) {
			setSourceError(failure instanceof Error ? failure.message : String(failure));
		}
	};

	const idWarning = (() => {
		if (origin === "bundled") return t("editor.quick.idBundled");
		const bundled = games.some((game) => game.id === draft.id.trim() && game.origin === "bundled");
		if (bundled) return t("editor.quick.idBundled");
		const taken = !editing && games.some((game) => game.id === draft.id.trim() && game.origin === "user");
		return taken ? t("editor.quick.idTaken") : null;
	})();
	const issues = localIssues(draft);
	const blocked = saving || idWarning !== null || check === null || !check.ok;
	const definitionsFolder = dataFolder === null ? null : `${dataFolder}/definitions`;
	const hasUserDefinitions = games.some((game) => game.origin === "user");
	const openDefinitionsFolder = () => {
		if (definitionsFolder !== null) void openFolderPath(definitionsFolder);
	};

	const exportDefinition = async () => {
		if (id === null) return;
		const path = await pickSaveFile(`${id}.zip`, t("settings.game.exportFilter"), "zip");
		if (path === null) return;
		await attempt(async () => {
			await unwrap(commands.exportDefinition(id, path));
			notify(t("notice.definitionExported", { path }));
		});
	};

	const save = async () => {
		if (blocked) return;
		if (sourceDirty) {
			try {
				parseDraft(source);
			} catch (failure) {
				setSourceError(failure instanceof Error ? failure.message : String(failure));
				return;
			}
		}
		const text = sourceDirty ? source : activeText;
		setSaving(true);
		await attempt(async () => {
			const saved = await unwrap(commands.saveDefinition(text, editing));
			setDirty(false);
			setShellDirty(false);
			onClose();
			await reloadGames();
			if (!editing && folder.trim()) {
				await gameActions.addGame(saved.id, folder.trim());
			} else {
				notify(t("editor.notice.saved", { name: saved.name }));
			}
		});
		setSaving(false);
	};

	return {
		editing,
		loaded,
		loadError,
		draft,
		folder,
		setFolder: changeFolder,
		inspection,
		inspecting,
		inspectError,
		presets,
		presetError,
		check,
		checkError,
		layout,
		issues,
		idWarning,
		blocked,
		saving,
		source,
		sourceDirty,
		formText,
		sourceError,
		activeText,
		dirty,
		origin,
		definitionsFolder,
		hasUserDefinitions,
		openDefinitionsFolder,
		update,
		chooseLayout,
		browse,
		changeSource,
		applySource,
		save,
		exportDefinition,
		requestLeave,
		onClose,
	};
}

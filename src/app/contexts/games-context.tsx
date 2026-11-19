import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useSettingsActions } from "@/app/contexts/settings-context";
import { commands, type DefinitionProblem_Serialize, type GameScan, type GameView, type ManagedGame } from "@/bindings";
import { messageOf, unwrap } from "@/lib/ipc";

interface Games {
	/// Every game the definitions know about, whether it is added or not.
	games: GameView[];
	/// The games added to Gobbo, or `null` until the first answer arrives.
	managed: ManagedGame[] | null;
	/// The added games, ready for the rail and the palette.
	managedGames: GameView[];
	/// The scan results, or `null` until the Games page asks for them.
	scans: GameScan[] | null;
	/// The failure of the last scan, so the Games page can offer a retry instead of a permanent "Searching…".
	scanError: string | null;
	/// User definitions that could not load. The rest of the catalog still works; the Games page names the files.
	definitionProblems: DefinitionProblem_Serialize[];
	/// The failure of the game catalog itself, shown with a retry that reloads the definitions.
	catalogError: string | null;
	scannedAt: Date | null;
	/// A scan is in flight. The button waits for it instead of starting a second one.
	scanning: boolean;
	setManaged: (games: ManagedGame[]) => void;
	scanGames: () => void;
	/// Reads the catalog and the added games again, after a failure or a change outside Gobbo.
	reloadGames: () => Promise<void>;
	/// Checks a folder for the Locate dialog. Errors go to the dialog itself, so this does not use the notice list.
	checkGameFolder: (gameId: string, gameRoot: string) => Promise<string | null>;
}

const GamesContext = createContext<Games | null>(null);

export function useGames(): Games {
	const context = useContext(GamesContext);
	if (!context) {
		throw new Error("useGames must be used inside <GamesProvider>.");
	}

	return context;
}

/// The game catalogue and the games the user added. The backend is the source of truth; this provider only keeps the
/// last answer and the scan that the Games page asks for.
export function GamesProvider({ children }: { children: ReactNode }) {
	const [games, setGames] = useState<GameView[]>([]);
	const [managed, setManaged] = useState<ManagedGame[] | null>(null);
	const [scans, setScans] = useState<GameScan[] | null>(null);
	const [scanError, setScanError] = useState<string | null>(null);
	const [definitionProblems, setDefinitionProblems] = useState<DefinitionProblem_Serialize[]>([]);
	const [catalogError, setCatalogError] = useState<string | null>(null);
	const [scannedAt, setScannedAt] = useState<Date | null>(null);
	const [scanning, setScanning] = useState(false);
	const scanningRef = useRef(false);
	const { notifyFailure, attempt } = useNoticeActions();
	const { loadAppSettings } = useSettingsActions();
	const { navigate } = useNavigation();

	/// On startup we read the supported games and the added list first. Settings are independent: a broken settings file
	/// must not leave the whole rail empty. A failed read stays on the Games page with its own retry.
	const reloadGames = useCallback(async () => {
		try {
			const [catalog, added] = await Promise.all([unwrap(commands.listGames()), unwrap(commands.managedGames())]);
			setGames(catalog.games);
			setDefinitionProblems(catalog.problems);
			setManaged(added);
			setCatalogError(null);
			if (added.length === 0) navigate("games");
		} catch (failure) {
			setCatalogError(messageOf(failure));
		}
	}, [navigate]);

	useEffect(() => {
		// oxlint-disable-next-line react/set-state-in-effect -- the catalogue is external state; this effect starts its first read.
		void reloadGames();
		void (async () => {
			try {
				loadAppSettings(await unwrap(commands.appSettings()));
			} catch (failure) {
				notifyFailure(failure);
			}
		})();
	}, [loadAppSettings, notifyFailure, reloadGames]);

	const scanGames = useCallback(() => {
		if (scanningRef.current) return;
		scanningRef.current = true;
		setScanning(true);
		setScanError(null);
		void attempt(async () => {
			try {
				const result = await unwrap(commands.scanGames());
				setScans(result);
				setScannedAt(new Date());
			} catch (failure) {
				scanningRef.current = false;
				setScanning(false);
				setScanError(messageOf(failure));
				throw failure;
			}
			scanningRef.current = false;
			setScanning(false);
		});
	}, [attempt]);

	const checkGameFolder = useCallback((gameId: string, gameRoot: string) => unwrap(commands.checkGameFolder(gameId, gameRoot)), []);

	const managedGames = useMemo(() => games.filter((game) => managed?.some((entry) => entry.id === game.id)), [games, managed]);

	const value = useMemo(
		(): Games => ({
			games,
			managed,
			managedGames,
			scans,
			scanError,
			definitionProblems,
			catalogError,
			scannedAt,
			scanning,
			setManaged,
			scanGames,
			reloadGames,
			checkGameFolder,
		}),
		[
			checkGameFolder,
			catalogError,
			definitionProblems,
			games,
			managed,
			managedGames,
			reloadGames,
			scanError,
			scanGames,
			scannedAt,
			scanning,
			scans,
		],
	);

	return <GamesContext.Provider value={value}>{children}</GamesContext.Provider>;
}

import { useEffect, useMemo, useRef, useState } from "react";
import { commands, type Status, type UpdateReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import { detailOf, errorText, messageOf, unwrap } from "@/lib/ipc";
import { updatesOf } from "@/lib/mods";
import { withoutKey } from "@/lib/records";

interface UseModUpdatesOptions {
	gameId: string | null;
	/// The status of the shown game. Each mod in it carries the result of its last saved check.
	status: Status | null;
	onStatus: (gameId: string, status: Status) => void;
	/// The display name of a game, for a result that arrives after the user switched away.
	nameOf: (gameId: string) => string;
	notify: (message: string) => void;
	notifyError: (message: string, detail?: string) => void;
}

const NO_FAILURES: Record<string, UpdateReport> = {};

/// Tracks which games have a bulk check in flight. Checks of different games may run side by side; a second check of
/// the same game is refused. The newest game is first, so the active game is `active()` when it checks. Pure and free of
/// Tauri calls so the concurrency rule can be tested on its own.
export function createCheckRegistry() {
	const games: string[] = [];
	return {
		start(gameId: string): boolean {
			if (games.includes(gameId)) return false;
			games.unshift(gameId);
			return true;
		},
		finish(gameId: string) {
			const index = games.indexOf(gameId);

			if (index !== -1) games.splice(index, 1);
		},
		active(): string | null {
			return games[0] ?? null;
		},
		list(): string[] {
			return [...games];
		},
	};
}

/// Update checks against Nexus. The state each mod shows is the result of its last saved check, read from the status:
/// the library keeps it, so a restart, a game switch or a new install shows it again. Only a failed check lives in this
/// session, and it is kept per game so switching games no longer drops another game's failures. Checks of different
/// games run side by side; only a duplicate check of the same game is refused. Apart from the mod a Nexus download has
/// just installed, a check is never started automatically; the Nexus budget is the user's.
export function useModUpdates({ gameId, status, onStatus, nameOf, notify, notifyError }: UseModUpdatesOptions) {
	const { t } = useTranslation();
	/// Failed checks of this session, keyed by game then mod id. A check that works again removes its failure.
	const [failuresByGame, setFailuresByGame] = useState<Record<string, Record<string, UpdateReport>>>({});
	/// The games whose bulk check is running, newest first. The derived `checkingGame` stays the active game's value
	/// when it checks, and `checkingGames` lets the activity hook show checks of other games too.
	const [checkingGames, setCheckingGames] = useState<string[]>([]);
	/// The same flags synchronously, so a second click in the same tick cannot start a second check for one game.
	const checks = useRef(createCheckRegistry());
	/// The game the results belong to. A check started for one game must not write into the next game's maps.
	const activeGame = useRef(gameId);
	useEffect(() => {
		activeGame.current = gameId;
	}, [gameId]);
	/// Bumped when a failure is forgotten: a check that was already in flight must not put it back.
	const generation = useRef(0);

	const activeKey = gameId ?? "";
	const failures = failuresByGame[activeKey] ?? NO_FAILURES;
	/// The active game's check when it has one, otherwise the most recently started one. A check that outlives a game
	/// switch still has a value, so the activity card does not silently lose it.
	const checkingGame = checkingGames[0] ?? null;

	/// What the table shows for each mod: its saved check, or the failure of this session when its last check failed.
	const updates = useMemo(() => updatesOf(status?.mods ?? [], failures), [status, failures]);

	/// Keeps the failures of the checks that just ran for their own game, and drops the failure of a mod that checked again.
	const record = (recordGame: string, reports: UpdateReport[]) => {
		setFailuresByGame((prev) => {
			const current = prev[recordGame] ?? NO_FAILURES;
			let next = current;

			for (const report of reports) {
				if (report.error) next = { ...next, [report.id]: report };
				else if (report.id in next) next = withoutKey(next, report.id);
			}

			if (next === current) return prev;
			return { ...prev, [recordGame]: next };
		});
	};

	/// Checks one mod and stores the result. The caller decides how to show a failure.
	const checkMod = async (modId: string): Promise<UpdateReport> => {
		if (!gameId) throw new Error(t("import.noGame"));
		const requestedGame = gameId;
		const startedAt = generation.current;
		const report = await unwrap(commands.checkModUpdate(requestedGame, modId));

		if (activeGame.current !== requestedGame || generation.current !== startedAt) return report;
		record(requestedGame, [report]);
		// A successful check is saved in the library, so the status carries the new result. A failed refresh is a
		// status problem, not a failed check: the report is still valid.
		if (!report.error) {
			try {
				onStatus(gameId, await unwrap(commands.getStatus(gameId)));
			} catch (failure) {
				notifyError(messageOf(failure), detailOf(failure));
			}
		}

		return report;
	};

	/// The context menu has no inspector to show an inline failure, so the result goes to a notice.
	const checkModFromMenu = async (modId: string) => {
		try {
			const report = await checkMod(modId);

			if (report.error) {
				notifyError(errorText(report.error));
			} else if (report.state === "ignored" && report.latest) {
				notify(t("mods.version.ignored", { version: report.latest }));
			} else if (report.has_update && report.latest) {
				if (report.latest_installed) notify(t("mods.version.installed", { version: report.latest }));
				else notify(t("mods.version.updateAvailable", { version: report.latest }));
			} else {
				notify(t("mods.version.upToDate"));
			}
		} catch (failure) {
			notifyError(messageOf(failure), detailOf(failure));
		}
	};

	/// Checks every mod of the active profile that has a Nexus ID. Only the user asks for it, never at startup. A
	/// check of another game is independent: only a second check of the same game is refused.
	const checkUpdates = async () => {
		if (!gameId) return;
		const requestedGame = gameId;

		if (!checks.current.start(requestedGame)) return;
		const startedAt = generation.current;
		setCheckingGames(checks.current.list());
		/// Cleared on both paths; never in a `finally`, which the React Compiler may skip.
		const finish = () => {
			checks.current.finish(requestedGame);
			setCheckingGames(checks.current.list());
		};

		try {
			const reports = await unwrap(commands.checkUpdates(requestedGame));
			finish();
			const switched = activeGame.current !== requestedGame || generation.current !== startedAt;

			if (!switched) {
				record(requestedGame, reports);

				if (reports.some((report) => !report.error)) {
					try {
						onStatus(requestedGame, await unwrap(commands.getStatus(requestedGame)));
					} catch (failure) {
						notifyError(messageOf(failure), detailOf(failure));
					}
				}
			}

			const available = reports.filter((report) => report.has_update).length;
			const ignored = reports.filter((report) => report.state === "ignored").length;
			const broken = reports.filter((report) => report.error).length;
			let base = t("updates.allCurrent", { count: reports.length });

			if (available > 0) base = t("updates.found", { count: available, total: reports.length });
			else if (ignored > 0) base = t("updates.noNewVersions", { count: reports.length });
			const ignoredText = ignored > 0 ? ` ${t("updates.ignored", { count: ignored })}` : "";
			const brokenText = broken > 0 ? ` ${t("updates.broken", { count: broken })}` : "";
			const summary = `${base}${ignoredText}${brokenText}`;
			notify(switched ? t("updates.finishedFor", { game: nameOf(requestedGame), summary }) : summary);
		} catch (failure) {
			finish();
			notifyError(messageOf(failure), detailOf(failure));
		}
	};

	/// A changed Nexus ID or version: the failure of this session is about the old state, so it is dropped. The saved
	/// check follows the library, and the status shows it again with the version the profile now selects. The optional
	/// game id lets a background action forget its own failure while another game is shown.
	const forget = (modId: string, forgetGame: string = gameId ?? "") => {
		generation.current += 1;
		setFailuresByGame((prev) => {
			const current = prev[forgetGame] ?? NO_FAILURES;

			if (!(modId in current)) return prev;
			return { ...prev, [forgetGame]: withoutKey(current, modId) };
		});
	};

	/// A different profile or list can change every version, so every earlier failure of that game is stale.
	const forgetAll = (allGame: string = gameId ?? "") => {
		generation.current += 1;
		setFailuresByGame((prev) => {
			if (!(allGame in prev)) return prev;
			return withoutKey(prev, allGame);
		});
	};

	return { updates, checkingGame, checkingGames, checkMod, checkModFromMenu, checkUpdates, forget, forgetAll };
}

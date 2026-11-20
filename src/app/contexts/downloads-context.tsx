import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { commands } from "@/bindings";
import { useTranslation } from "@/i18n";
import { errorText, unwrap } from "@/lib/ipc";
import { registerTask, removeTask } from "@/lib/tasks-store";
import type { useImport } from "@/lib/use-import";
import { type Download, type DownloadImported, downloadSummary, useDownloads } from "@/nexus/use-downloads";

interface Downloads {
	downloads: Download[];
	/// What the rail button shows: running, progress and waiting for an install.
	summary: ReturnType<typeof downloadSummary>;
	installDownload: (download: Download) => void | Promise<void>;
	removeDownload: (downloadId: number) => void | Promise<void>;
	retryDownload: (downloadId: number) => void | Promise<void>;
	clearDownloads: () => void | Promise<void>;
}

const DownloadsContext = createContext<Downloads | null>(null);

export function useDownloadsContext(): Downloads {
	const context = useContext(DownloadsContext);

	if (!context) {
		throw new Error("useDownloads must be used inside <DownloadsProvider>.");
	}

	return context;
}

/// The provider download queue. Kept below the workspace so a progress event re-renders the rail and the downloads page,
/// not the mod table or the inspector.
export function DownloadsProvider({
	children,
	importPaths,
	acceptImported,
}: {
	children: ReactNode;
	importPaths: ReturnType<typeof useImport>["importPaths"];
	acceptImported: (imported: DownloadImported) => void;
}) {
	const { t } = useTranslation();
	const { notifyError, attempt } = useNoticeActions();

	const { downloads, setDownloads } = useDownloads({
		// A start is ongoing work, which the live Working row already shows. No toast and no persisted notice for it.
		onStarted: () => {
			// Intentionally empty: the Working row is the only feedback for a start.
		},
		onImported: acceptImported,
		onFailed: (download) =>
			notifyError(
				t("notice.downloadFailed", {
					name: download.name,
					error: download.error ? errorText(download.error) : "",
				}),
			),
	});

	/// The backend queue answers with a fresh list; the provider never patches it by hand.
	const installDownload = useCallback(
		(download: Download) => {
			void attempt(async () => {
				if (download.path)
					await importPaths([download.path], {
						game: download.game,
						source: download.source,
					});
			});
		},
		[attempt, importPaths],
	);
	/// A FOMOD wizard is a user action, not an effect: claim first so the backend drops its durable wait and a second
	/// window cannot open the same wizard, then run the same import path as a manual install.
	const installWizard = useCallback(
		(download: Download) => {
			void attempt(async () => {
				const claimed = await commands.claimDownload(download.id);

				if (claimed?.path) {
					// The backend has dropped its wait; reflect that at once so a cancelled wizard can be retried from
					// the ordinary waiting install row instead of leaving a dead wizard row behind.
					setDownloads((queue) => queue.map((item) => (item.id === download.id ? { ...item, auto_install: false } : item)));
					await importPaths([claimed.path], {
						game: claimed.game,
						source: claimed.source,
					});
				}
			});
		},
		[attempt, importPaths, setDownloads],
	);

	/// The backend keeps the wizard wait with the queue, so a restart still finds it. Mirror each pending archive into
	/// the global Working panel as a waiting row with an Open action; the queue answer is the only source of truth.
	const wizardTasks = useRef<Set<string>>(new Set());
	useEffect(() => {
		const live = new Set<string>();

		for (const download of downloads) {
			if (download.state !== "ready" || !download.auto_install) continue;
			const id = `download-wizard:${download.id}`;
			live.add(id);
			registerTask({
				id,
				game: download.game || null,
				kind: "import",
				title: t("tasks.waitingInstall", { name: download.name }),
				state: "waiting",
				open: {
					label: t("downloads.install"),
					run: () => installWizard(download),
				},
				source: "local",
			});
		}

		for (const id of wizardTasks.current) {
			if (!live.has(id)) removeTask(id);
		}

		wizardTasks.current = live;
	}, [downloads, installWizard, t]);
	const removeDownload = useCallback(
		(downloadId: number) => {
			void attempt(async () => {
				setDownloads(await unwrap(commands.removeDownload(downloadId)));
			});
		},
		[attempt, setDownloads],
	);
	const retryDownload = useCallback(
		(downloadId: number) => {
			void attempt(async () => {
				setDownloads(await commands.retryDownload(downloadId));
			});
		},
		[attempt, setDownloads],
	);
	const clearDownloads = useCallback(() => {
		void attempt(async () => {
			setDownloads(await commands.clearDownloads());
		});
	}, [attempt, setDownloads]);

	const summary = useMemo(() => downloadSummary(downloads), [downloads]);
	const value = useMemo(
		(): Downloads => ({
			downloads,
			summary,
			installDownload,
			removeDownload,
			retryDownload,
			clearDownloads,
		}),
		[clearDownloads, downloads, installDownload, removeDownload, retryDownload, summary],
	);

	return <DownloadsContext.Provider value={value}>{children}</DownloadsContext.Provider>;
}

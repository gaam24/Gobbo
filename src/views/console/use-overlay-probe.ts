import { useState } from "react";
import { commands, type OverlayProbeReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { runBusyAction } from "@/lib/use-busy-action";

/// Runs the overlay doctor on demand and opens the overlay state folder.
export function useOverlayProbe(gameId: string) {
	const { t } = useTranslation();
	const [reports, setReports] = useState<OverlayProbeReport[] | null>(null);
	const [failure, setFailure] = useState<string | null>(null);
	const [running, setRunning] = useState(false);

	const run = async () => {
		setReports(null);
		await runBusyAction(
			async () => {
				setReports(await unwrap(commands.runOverlayProbe(gameId)));
			},
			setRunning,
			(error) => setFailure(messageOf(error)),
			() => setFailure(null),
		);
	};

	const openFolder = async () => {
		setFailure(null);

		try {
			const path = await unwrap(commands.overlayState(gameId));

			if (path === null) {
				setFailure(t("console.probeNoOverlay"));
				return;
			}

			await unwrap(commands.openPath(gameId, path, false));
		} catch (error) {
			setFailure(messageOf(error));
		}
	};

	return { reports, failure, running, run, openFolder };
}

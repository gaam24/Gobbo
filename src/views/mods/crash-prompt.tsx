import { CircleAlert } from "lucide-react";
import { useState } from "react";

import { commands } from "@/bindings";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// After a launch that ended with a nonzero exit code and was not stopped by the user, offer the existing guided
/// search with the same enabled list. The prompt is tied to the report's start time, so a new launch can ask again.
export function CrashPrompt({
	gameId,
	gameRunning,
	bisectRunning,
	onFindMod,
}: {
	gameId: string;
	gameRunning: boolean;
	bisectRunning: boolean;
	onFindMod: () => void;
}) {
	const { t } = useTranslation();
	const [dismissed, setDismissed] = useState<number | null>(null);
	const report = useLoad(`${gameId}:${gameRunning}`, () => unwrap(commands.lastRun(gameId)));

	if (gameRunning || bisectRunning || !report) return null;
	const endedBadly = report.game_started && report.finished_at !== null && !report.stopped_by_user && report.game_exit_code !== 0;

	if (!endedBadly || dismissed === report.started_at) return null;

	return (
		<Alert className="mx-6 mt-4 w-auto min-w-0">
			<CircleAlert aria-hidden />
			<AlertTitle>{t("crashPrompt.title")}</AlertTitle>
			<AlertDescription>{t("crashPrompt.body")}</AlertDescription>
			<AlertAction className="flex gap-2">
				<Button variant="outline" size="sm" onClick={() => setDismissed(report.started_at)}>
					{t("crashPrompt.notNow")}
				</Button>
				<Button size="sm" onClick={onFindMod}>
					{t("crashPrompt.find")}
				</Button>
			</AlertAction>
		</Alert>
	);
}

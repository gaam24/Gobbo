import { X } from "lucide-react";
import { type ReactNode, useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";
import { LaunchBody } from "@/views/console/launch-body";

/// How often the open console reads the last launch again.
const REFRESH_MS = 2000;

/// The console under the mod list: what the game wrote in its last launch from Gobbo, how the game ended, and the mod
/// steps that ran before it with their logs. It refreshes while it is open.
export function GameConsole({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	/// `undefined` while the first answer is pending, `null` when the game was never launched from Gobbo. Without the
	/// backend the panel shows nothing, as in a browser preview.
	const report = useLoad(gameId, () => unwrap(commands.lastRun(gameId)), {
		pollMs: REFRESH_MS,
		onError: (error) => setFailure(messageOf(error)),
		onSuccess: () => setFailure(null),
	});

	let body: ReactNode = null;
	if (report !== undefined && report !== null) {
		body = <LaunchBody report={report} gameId={gameId} />;
	}

	return (
		<section id="game-console" aria-label={t("console.title")} className="bg-card flex h-full min-h-0 flex-col">
			<div className="flex h-10 shrink-0 items-center justify-between gap-3 border-b px-6">
				<h2 className="text-sm font-medium">{t("console.title")}</h2>
				<Button variant="ghost" size="icon-sm" aria-label={t("console.close")} onClick={onClose}>
					<X />
				</Button>
			</div>
			<div className="flex min-h-0 flex-1 flex-col">
				{failure && (
					<p role="alert" className="text-destructive shrink-0 px-6 pt-3 text-xs">
						{t("console.stale", { error: failure })}
					</p>
				)}
				{report === null ? <p className="text-muted-foreground px-6 py-3 text-sm">{t("runs.none")}</p> : body}
			</div>
		</section>
	);
}

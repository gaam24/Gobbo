import { useState } from "react";
import { commands, type OverlayProbeReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";

/// Runs the overlay doctor on demand and lists its checks for x64 and x86.
export function OverlayProbe({ gameId }: { gameId: string }) {
	const { t } = useTranslation();
	const [reports, setReports] = useState<OverlayProbeReport[] | null>(null);
	const [failure, setFailure] = useState<string | null>(null);
	const [running, setRunning] = useState(false);

	const run = async () => {
		setRunning(true);
		setFailure(null);
		setReports(null);
		try {
			setReports(await unwrap(commands.runOverlayProbe(gameId)));
		} catch (error) {
			setFailure(messageOf(error));
		}
		setRunning(false);
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

	return (
		<section aria-labelledby="console-probe" className="grid gap-2">
			<div className="flex items-center justify-between gap-3">
				<h3 id="console-probe" className="text-sm font-medium">
					{t("console.probe")}
				</h3>
				<div className="flex items-center gap-2">
					<Button variant="ghost" size="sm" onClick={() => void openFolder()}>
						{t("console.probeOpen")}
					</Button>
					<Button variant="outline" size="sm" disabled={running} onClick={() => void run()}>
						{running ? t("console.probeRunning") : t("console.probeRun")}
					</Button>
				</div>
			</div>
			<p className="text-muted-foreground text-xs">{t("console.probeHint")}</p>
			{failure && <p className="text-destructive text-xs">{failure}</p>}
			{reports?.map((report) => (
				<div key={report.arch} className="grid gap-1">
					<p className="flex flex-wrap gap-2 text-xs">
						<span className="font-medium">{report.arch}</span>
						<span className={report.ok ? "text-muted-foreground" : "text-destructive"}>
							{report.ok ? t("console.probeOk") : t("console.probeFailed")}
						</span>
						{report.probe && <span className="text-muted-foreground font-mono break-all">{report.probe}</span>}
					</p>
					<ul className="grid gap-1 font-mono text-xs break-all">
						{report.checks.map((check) => (
							<li key={`${report.arch}-${check.name}`} className="flex flex-wrap gap-2">
								<span className={check.ok ? "text-muted-foreground" : "text-destructive"}>
									{check.ok ? t("console.probeOk") : t("console.probeFailed")}
								</span>
								<span>{check.name}</span>
								{!check.ok && check.detail && <span className="text-destructive">{check.detail}</span>}
							</li>
						))}
					</ul>
				</div>
			))}
		</section>
	);
}

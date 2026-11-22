import { useState } from "react";
import type { RunReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { useTranslation } from "@/i18n";
import { useCopyLines } from "@/lib/use-copy-lines";

/// The newest decisions of the overlay hook, oldest first: writes, copy-ups, whiteouts, missing files, refusals and
/// pass-through writes. A decision names the layer and the path, so a game that fails to find or write a file shows why.
export function OverlayDecisions({ report }: { report: RunReport }) {
	const decisions = report.decisions ?? [];
	const total = report.decisions_total ?? 0;
	const [copiedKey, setCopiedKey] = useState<string | null>(null);
	const { t } = useTranslation();
	const newest = decisions.at(-1);
	const newestKey = newest ? `${newest.pid}-${newest.seq}` : null;
	const copied = copiedKey !== null && copiedKey === newestKey;
	const kindText: Record<string, string> = {
		write: t("console.decisionKind.write"),
		copy_up: t("console.decisionKind.copy_up"),
		whiteout: t("console.decisionKind.whiteout"),
		missing: t("console.decisionKind.missing"),
		refuse: t("console.decisionKind.refuse"),
		pass_through: t("console.decisionKind.pass_through"),
		list: t("console.decisionKind.list"),
	};
	const layerText: Record<string, string> = {
		none: t("console.decisionLayer.none"),
		upper: t("console.decisionLayer.upper"),
		staging: t("console.decisionLayer.staging"),
		root: t("console.decisionLayer.root"),
	};
	const { copyLines } = useCopyLines();
	const copy = async () => {
		const lines = decisions.map((decision) => {
			const error = decision.error === 0 ? "" : ` 0x${decision.error.toString(16).toUpperCase()}`;
			const at = new Date(decision.time * 1000).toISOString();
			return `${at} ${decision.kind} ${decision.layer} ${decision.path}${error}`;
		});
		await copyLines(lines, () => setCopiedKey(newestKey));
	};

	return (
		<section aria-labelledby="console-decisions" className="grid gap-2">
			<div className="flex items-center justify-between gap-3">
				<h3 id="console-decisions" className="text-sm font-medium">
					{t("console.decisions")}
				</h3>
				{decisions.length > 0 && (
					<Button variant="ghost" size="sm" onClick={() => void copy()}>
						{copied ? t("console.decisionsCopied") : t("console.decisionsCopy")}
					</Button>
				)}
			</div>
			{total === 0 ? (
				<Empty className="border border-dashed">
					<EmptyHeader>
						<EmptyDescription>{t("console.decisionsEmpty")}</EmptyDescription>
					</EmptyHeader>
				</Empty>
			) : (
				<p className="text-muted-foreground text-xs">{t("console.decisionsSummary", { count: total })}</p>
			)}
			<ul className="grid gap-1 font-mono text-xs break-all">
				{decisions.map((decision) => (
					<li key={`${decision.pid}-${decision.seq}`} className="flex flex-wrap gap-2">
						<span className="text-muted-foreground shrink-0">{kindText[decision.kind] ?? decision.kind}</span>
						<span className="text-muted-foreground shrink-0">{layerText[decision.layer] ?? decision.layer}</span>
						<span>{decision.path}</span>
						{decision.error !== 0 && (
							<span className="text-muted-foreground shrink-0">{`0x${decision.error.toString(16).toUpperCase()}`}</span>
						)}
					</li>
				))}
			</ul>
			{total > decisions.length && (
				<p className="text-muted-foreground text-xs">{t("console.decisionsShown", { shown: decisions.length, total })}</p>
			)}
			{report.crash_dump && (
				<p className="flex flex-wrap gap-2 font-mono text-xs break-all">
					<span className="text-muted-foreground">{t("console.crashDump")}</span>
					<span>{report.crash_dump}</span>
				</p>
			)}
		</section>
	);
}

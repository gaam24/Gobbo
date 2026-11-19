import type { ChangeKind, RunReport } from "@/bindings";
import { useTranslation } from "@/i18n";

/// What changed in the game folder during an overlay session: writes that got past the layers.
export function GameChanges({ report }: { report: RunReport }) {
	const changes = report.game_changes ?? [];
	const total = report.game_changes_total ?? 0;
	const { t } = useTranslation();
	const kindText: Record<ChangeKind, string> = {
		created: t("console.gameChangeCreated"),
		modified: t("console.gameChangeModified"),
		removed: t("console.gameChangeRemoved"),
	};

	return (
		<section aria-labelledby="console-game-changes" className="grid gap-2">
			<h3 id="console-game-changes" className="text-sm font-medium">
				{t("console.gameChanges")}
			</h3>
			<p className="text-muted-foreground text-xs">{t("console.gameChangesSummary", { count: total })}</p>
			<ul className="grid gap-1 font-mono text-xs break-all">
				{changes.map((change) => (
					<li key={change.path} className="flex flex-wrap gap-2">
						<span className="text-muted-foreground shrink-0">{kindText[change.kind]}</span>
						<span>{change.path}</span>
					</li>
				))}
			</ul>
			{total > changes.length && (
				<p className="text-muted-foreground text-xs">{t("console.gameChangesShown", { shown: changes.length, total })}</p>
			)}
		</section>
	);
}

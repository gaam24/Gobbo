import { cn } from "cn";
import { useState } from "react";
import type { RunReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { formatTime } from "@/lib/mods";
import { fileName } from "@/lib/paths";

/// What the list and the detail pane share. The key is stable across report refreshes, so a selected log stays
/// selected when the game writes another line.
interface LogEntry {
	key: string;
	group: "gobbo" | "game" | "mod";
	kind: string;
	path: string;
	lines: string[];
	modifiedAt?: number | null;
}

function parentName(path: string): string {
	const parts = path.split("/");
	return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
}

/// The picker on the left. A group is skipped when it has no files, so a game without a defined log does not show an
/// empty heading.
function LogGroup({
	title,
	entries,
	selected,
	onSelect,
}: {
	title: string;
	entries: LogEntry[];
	selected: string | null;
	onSelect: (key: string) => void;
}) {
	if (entries.length === 0) return null;
	return (
		<div className="grid gap-1">
			<h3 className="text-subtle-foreground px-2 text-xs font-medium tracking-wider uppercase">{title}</h3>
			{entries.map((entry) => {
				const active = entry.key === selected;
				return (
					<Button
						key={entry.key}
						variant="ghost"
						aria-current={active ? "true" : undefined}
						className={cn(
							"grid h-auto w-full min-w-0 justify-start gap-0.5 rounded-md px-2 py-1.5 text-left font-normal",
							active && "bg-muted",
						)}
						onClick={() => onSelect(entry.key)}
					>
						<span className="truncate font-mono text-xs">{fileName(entry.path)}</span>
						{parentName(entry.path) && <span className="text-subtle-foreground truncate font-mono text-xs">{parentName(entry.path)}</span>}
					</Button>
				);
			})}
		</div>
	);
}

/// The list of log files on the left and the selected file on the right. The game's standard output has its own tab;
/// this one holds Gobbo's log, the game logs named by the definition and every `.log` the mods write.
export function GameLogs({ report }: { report: RunReport }) {
	const { t, i18n } = useTranslation();
	const [selectedKey, setSelectedKey] = useState<string | null>(null);
	const [copied, setCopied] = useState(false);
	const kindText: Record<string, string> = {
		script: t("console.logKind.script"),
		loader: t("console.logKind.loader"),
		console: t("console.logKind.console"),
		crash: t("console.logKind.crash"),
		mod: t("console.logKind.mod"),
		game: t("console.logKind.game"),
	};

	const entries: LogEntry[] = [];

	if ((report.app_log ?? []).length > 0) {
		entries.push({
			key: "gobbo:gobbo.log",
			group: "gobbo",
			kind: "game",
			path: "gobbo.log",
			lines: report.app_log ?? [],
		});
	}

	for (const log of report.game_logs ?? []) {
		entries.push({
			key: `game:${log.path}`,
			group: "game",
			kind: log.kind,
			path: log.path,
			lines: log.lines,
			modifiedAt: log.modified_at,
		});
	}

	for (const log of report.mod_logs ?? []) {
		entries.push({
			key: `mod:${log.path}`,
			group: "mod",
			kind: log.kind,
			path: log.path,
			lines: log.lines,
			modifiedAt: log.modified_at,
		});
	}

	const selected = entries.find((entry) => entry.key === selectedKey) ?? entries[0];
	const select = (key: string) => {
		setSelectedKey(key);
		setCopied(false);
	};
	const copy = async () => {
		if (!selected) return;

		try {
			await navigator.clipboard.writeText(selected.lines.join("\n"));
			setCopied(true);
		} catch {
			// The clipboard needs a secure context; the lines stay selectable on screen.
		}
	};

	if (entries.length === 0) {
		return <p className="text-muted-foreground text-xs">{t("console.gameLogsEmpty")}</p>;
	}

	return (
		<div className="grid h-full min-h-64 gap-4 md:grid-cols-[minmax(12rem,18rem)_minmax(0,1fr)] md:grid-rows-[minmax(0,1fr)]">
			<div className="flex min-h-0 flex-col gap-4">
				<p className="text-muted-foreground shrink-0 text-xs">{t("console.logFilesHint")}</p>
				<nav aria-label={t("console.logFiles")} className="grid min-h-0 flex-1 content-start gap-4 overflow-y-auto pr-1">
					<LogGroup
						title={t("console.logGroupGobbo")}
						entries={entries.filter((entry) => entry.group === "gobbo")}
						selected={selected?.key ?? null}
						onSelect={select}
					/>
					<LogGroup
						title={t("console.logGroupGame")}
						entries={entries.filter((entry) => entry.group === "game")}
						selected={selected?.key ?? null}
						onSelect={select}
					/>
					<LogGroup
						title={t("console.logGroupMods")}
						entries={entries.filter((entry) => entry.group === "mod")}
						selected={selected?.key ?? null}
						onSelect={select}
					/>
				</nav>
			</div>

			{selected && (
				<section aria-label={fileName(selected.path)} className="flex min-h-0 flex-col gap-2">
					<div className="flex shrink-0 flex-wrap items-baseline justify-between gap-2">
						<div className="flex min-w-0 flex-wrap items-baseline gap-2">
							<h3 className="text-sm font-medium">{fileName(selected.path)}</h3>
							<span className="text-muted-foreground text-xs">
								{selected.group === "gobbo" ? t("console.logGroupGobbo") : (kindText[selected.kind] ?? selected.kind)}
							</span>
							{selected.modifiedAt != null && (
								<span className="text-subtle-foreground text-xs">{formatTime(selected.modifiedAt * 1000, i18n.language)}</span>
							)}
						</div>
						<Button variant="ghost" size="sm" onClick={() => void copy()}>
							{copied ? t("console.logCopied") : t("console.logCopy")}
						</Button>
					</div>
					<p className="text-muted-foreground shrink-0 truncate font-mono text-xs">{selected.path}</p>
					<pre className="bg-muted min-h-40 flex-1 overflow-auto rounded-lg border p-3 font-mono text-xs break-all whitespace-pre-wrap">
						{selected.lines.length > 0 ? selected.lines.join("\n") : t("console.logEmptyFile")}
					</pre>
				</section>
			)}
		</div>
	);
}

import { ArrowDown, ArrowDownRight, ArrowUp, ArrowUpRight } from "lucide-react";
import type { ModDetails as ModDetailsData, Status } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { ItemContent } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { modName } from "@/lib/mods";

/// Mods that share files with the selected one. Each row says who wins, with an arrow and a word, and goes to that mod.
export function ConflictsTab({
	modId,
	conflicts,
	status,
	onSelect,
	onMoveBeside,
}: {
	modId: string;
	conflicts: Map<string, { win: number; lose: number }>;
	status: Status;
	onSelect: (folders: string[]) => void;
	onMoveBeside: (modId: string, otherId: string, side: "above" | "below") => void | Promise<void>;
}) {
	const { t } = useTranslation();

	if (conflicts.size === 0) {
		return (
			<Empty className="border border-dashed">
				<EmptyHeader>
					<EmptyDescription>{t("mods.conflicts.none")}</EmptyDescription>
				</EmptyHeader>
			</Empty>
		);
	}

	return (
		<div className="grid gap-2">
			<p className="text-muted-foreground text-sm">{t("mods.conflicts.hint")}</p>
			<BoxList>
				{[...conflicts.entries()].map(([other, counts]) => {
					const otherName = status.mods.find((item) => item.id === other)?.info.name ?? other;
					return (
						<BoxListRow key={other} className="py-1.5">
							<div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
								<Button
									variant="ghost"
									className="h-auto min-w-0 flex-1 justify-between gap-3 px-2 py-1.5 text-left"
									onClick={() => onSelect([other])}
								>
									<span className="min-w-0 truncate font-medium">{otherName}</span>
									<span className="flex shrink-0 flex-col items-end gap-0.5 text-xs">
										{counts.win > 0 && (
											<span className="text-win flex items-center gap-1">
												<ArrowUpRight aria-hidden className="size-3.5" />
												{t("mods.conflicts.youWin", { count: counts.win })}
											</span>
										)}
										{counts.lose > 0 && (
											<span className="text-lose flex items-center gap-1">
												<ArrowDownRight aria-hidden className="size-3.5" />
												{t("mods.conflicts.youLose", { count: counts.lose })}
											</span>
										)}
									</span>
								</Button>
								<div className="flex shrink-0 gap-1">
									<Button
										variant="outline"
										size="sm"
										title={t("mods.conflicts.moveHint")}
										onClick={() => void onMoveBeside(modId, other, "above")}
									>
										<ArrowUp /> {t("mods.conflicts.moveAbove")}
									</Button>
									<Button
										variant="outline"
										size="sm"
										title={t("mods.conflicts.moveHint")}
										onClick={() => void onMoveBeside(modId, other, "below")}
									>
										<ArrowDown /> {t("mods.conflicts.moveBelow")}
									</Button>
								</div>
							</div>
						</BoxListRow>
					);
				})}
			</BoxList>
		</div>
	);
}

interface FileWinnersProps {
	modId: string;
	conflicts: ModDetailsData["conflicts"];
	status: Status;
	canChoose: boolean;
	onSetWinner: (path: string, familyId: string | null) => void | Promise<void>;
}

/// The files this mod shares with others, and which copy wins. Only `Mirror` lets Gobbo choose; elsewhere the game does.
export function FileWinners({ modId, conflicts, status, canChoose, onSetWinner }: FileWinnersProps) {
	const { t } = useTranslation();

	if (conflicts.length === 0) return null;
	const nameOf = (id: string) => modName(status.mods, id);

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.winners.title")} />
			<p className="text-muted-foreground text-xs">{canChoose ? t("mods.winners.hint") : t("mods.winners.slots")}</p>
			<BoxList>
				{conflicts.map((conflict) => {
					const wins = conflict.winner === modId;
					return (
						<BoxListRow key={conflict.path}>
							<ItemContent className="gap-1">
								<span className="truncate font-mono text-xs" title={conflict.path}>
									{conflict.path}
								</span>
								<span className="text-muted-foreground text-xs">
									{wins ? t("mods.winners.wins") : t("mods.winners.loses", { name: nameOf(conflict.winner) })}
								</span>
								{canChoose && (
									<div className="flex flex-wrap gap-2">
										<Button variant="outline" size="sm" disabled={wins} onClick={() => void onSetWinner(conflict.path, modId)}>
											{t("mods.winners.choose")}
										</Button>
										<Button variant="ghost" size="sm" onClick={() => void onSetWinner(conflict.path, null)}>
											{t("mods.winners.default")}
										</Button>
									</div>
								)}
							</ItemContent>
						</BoxListRow>
					);
				})}
			</BoxList>
		</section>
	);
}

import { Play, SearchCheck } from "lucide-react";
import type { BisectView } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";

/// The search for the mod that breaks the game, one test at a time: start the game with this half of the mods, then
/// say whether the problem is still there. At the end the list goes back as it was.
export function BisectBar({
	bisect,
	gameRunning,
	onLaunch,
	onAnswer,
	onStop,
}: {
	bisect: BisectView;
	gameRunning: boolean;
	onLaunch: () => void | Promise<void>;
	onAnswer: (broken: boolean) => void | Promise<void>;
	onStop: (disableFound: boolean) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const busy = gameRunning;

	return (
		<section role="status" aria-label={t("bisect.title")} className="bg-muted grid shrink-0 gap-2 border-b px-6 py-3 text-sm">
			<div className="flex flex-wrap items-center gap-2">
				<SearchCheck className="size-4 shrink-0" aria-hidden />
				<h2 className="font-semibold">{t("bisect.title")}</h2>
				{bisect.found === null ? (
					<span className="text-muted-foreground">
						{t("bisect.step", {
							step: bisect.step,
							left: bisect.steps_left,
							suspects: bisect.suspects,
						})}
					</span>
				) : (
					<span>{t("bisect.found", { name: bisect.found })}</span>
				)}
			</div>
			{bisect.found === null ? (
				<>
					<p className="text-muted-foreground">{t("bisect.instructions", { count: bisect.testing.length })}</p>
					<div className="flex flex-wrap gap-2">
						<Button size="sm" disabled={busy} onClick={() => void onLaunch()}>
							<Play /> {t("bisect.launch")}
						</Button>
						<Button size="sm" variant="outline" disabled={busy} onClick={() => void onAnswer(true)}>
							{t("bisect.broken")}
						</Button>
						<Button size="sm" variant="outline" disabled={busy} onClick={() => void onAnswer(false)}>
							{t("bisect.works")}
						</Button>
						<Button size="sm" variant="ghost" className="ml-auto" disabled={busy} onClick={() => void onStop(false)}>
							{t("bisect.stop")}
						</Button>
					</div>
				</>
			) : (
				<div className="flex flex-wrap gap-2">
					<Button size="sm" disabled={busy} onClick={() => void onStop(true)}>
						{t("bisect.disableAndFinish")}
					</Button>
					<Button size="sm" variant="outline" disabled={busy} onClick={() => void onStop(false)}>
						{t("bisect.finish")}
					</Button>
				</div>
			)}
		</section>
	);
}

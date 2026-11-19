import { Command, Keyboard, Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { useTranslation } from "@/i18n";

/// The thin bar at the bottom of the mod list: the count while a filter narrows the list, the game console, the command
/// palette and the shortcut list. Both keyboard-first actions sit here, where they take no room from the header.
export function ModsStatusBar({
	shown,
	total,
	consoleOpen,
	onConsoleOpen,
	onOpenPalette,
	onOpenShortcuts,
}: {
	shown: number;
	total: number;
	consoleOpen: boolean;
	onConsoleOpen: (open: boolean) => void;
	onOpenPalette: () => void;
	onOpenShortcuts: () => void;
}) {
	const { t } = useTranslation();

	return (
		<div className="text-muted-foreground flex h-9 shrink-0 items-center gap-4 overflow-hidden border-t px-6 text-xs whitespace-nowrap">
			<Button
				variant="ghost"
				size="sm"
				aria-expanded={consoleOpen}
				aria-controls="game-console"
				title={t("console.toggleHint")}
				onClick={() => onConsoleOpen(!consoleOpen)}
			>
				<Terminal /> {t("console.title")}
			</Button>
			{shown !== total && <span>{t("bar.showing", { shown, total })}</span>}
			<Button variant="ghost" size="sm" className="ml-auto shrink-0" title={t("palette.hint")} onClick={onOpenPalette}>
				<Command /> <span className="hidden @2xl:inline">{t("palette.open")}</span>
				<KbdGroup className="hidden @lg:inline-flex">
					<Kbd>{t("bar.keys.ctrl")}</Kbd>
					<Kbd>{"K"}</Kbd>
				</KbdGroup>
			</Button>
			<Button variant="ghost" size="sm" className="shrink-0" title={t("shortcuts.title")} onClick={onOpenShortcuts}>
				<Keyboard /> <span className="hidden @2xl:inline">{t("shortcuts.button")}</span>
				<Kbd className="hidden @lg:inline-flex">{"?"}</Kbd>
			</Button>
		</div>
	);
}

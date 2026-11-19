import { Trash2 } from "lucide-react";
import type { GameView, Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { Section, SectionBlock } from "@/views/settings/controls";

/// Removing a game from Gobbo does not delete its files or its mods; the hint says so before the confirmation opens.
export function RemoveSection({
	status,
	game,
	onRequestRemove,
}: {
	status: Status;
	game: GameView | undefined;
	onRequestRemove: () => void;
}) {
	const { t } = useTranslation();

	return (
		<Section id="settings-remove" title={t("settings.remove.title")} hint={t("settings.remove.hint", { name: status.game_name })} danger>
			<SectionBlock>
				<Button
					type="button"
					variant="outline"
					size="sm"
					className="text-destructive border-destructive hover:text-destructive justify-self-start"
					disabled={!game}
					onClick={onRequestRemove}
				>
					<Trash2 /> {t("games.remove")}
				</Button>
			</SectionBlock>
		</Section>
	);
}

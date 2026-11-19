import type { GameView } from "@/bindings";
import type { useTranslation } from "@/i18n";

/// The one line that names the test behind a verified definition. A user definition without it is unverified.
export function verifiedHint(t: ReturnType<typeof useTranslation>["t"], verified: NonNullable<GameView["verified"]>): string {
	const systems = verified.os ?? [];
	return t("games.verifiedHint", {
		date: verified.date,
		os: systems.length > 0 ? systems.join(", ") : "—",
		game: verified.game_version ?? "—",
		loader: verified.loader_version ?? "—",
	});
}

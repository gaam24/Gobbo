import { Download, Pencil } from "lucide-react";
import type { GameView } from "@/bindings";
import { Disclosure } from "@/components/app/content-blocks";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { verifiedHint } from "@/lib/game-definition";
import { Section, SectionBlock, SettingRow } from "@/views/settings/controls";

/// How the mods reach the game, and what the game definition says about it. The definition is read-only, so it stays
/// closed until someone asks for it.
export function AdvancedSection({
	game,
	onExportDefinition,
	onEditDefinition,
}: {
	game: GameView | undefined;
	/// Writes a shareable zip. Omitted in a screen that does not offer the action, such as the section test.
	onExportDefinition?: () => void | Promise<void>;
	/// Opens the definition editor. A bundled game is read-only, so the action is offered only for a user definition.
	onEditDefinition?: () => void | Promise<void>;
}) {
	const { t } = useTranslation();

	return (
		<Section id="settings-advanced" title={t("settings.advanced.title")} hint={t("settings.advanced.hint")}>
			<SettingRow title={t("settings.overlay.title")} help={t("settings.overlay.hint")}>
				<Badge variant="secondary">{"overlay"}</Badge>
			</SettingRow>

			<SectionBlock>
				<Disclosure title={t("settings.game.definition")}>
					<div className="grid gap-4">
						<p className="text-muted-foreground text-xs text-pretty">{t("settings.game.hint")}</p>
						<DefinitionFacts game={game} />
						{game && (onExportDefinition !== undefined || (game.origin === "user" && onEditDefinition !== undefined)) && (
							<div className="flex flex-wrap gap-2">
								{game.origin === "user" && onEditDefinition && (
									<Button variant="outline" size="sm" onClick={() => void onEditDefinition()}>
										<Pencil aria-hidden />
										{t("settings.game.editDefinition")}
									</Button>
								)}
								{onExportDefinition && (
									<Button variant="outline" size="sm" onClick={() => void onExportDefinition()}>
										<Download aria-hidden />
										{t("settings.game.exportDefinition")}
									</Button>
								)}
							</div>
						)}
					</div>
				</Disclosure>
			</SectionBlock>
		</Section>
	);
}

/// The facts of the game definition: id, origin, loader, Steam ids and the places it writes to.
export function DefinitionFacts({ game }: { game: GameView | undefined }) {
	const { t } = useTranslation();

	if (!game) return <p className="text-muted-foreground text-sm">{t("settings.game.missing")}</p>;
	return (
		<dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-3 text-sm">
			<dt className="text-muted-foreground">{t("settings.game.id")}</dt>
			<dd className="font-mono text-xs">{game.id}</dd>
			<dt className="text-muted-foreground">{t("settings.game.origin")}</dt>
			<dd className="flex flex-wrap items-center gap-1.5">
				<Badge variant={game.origin === "user" ? "secondary" : "outline"}>
					{t(game.origin === "user" ? "games.userDefinition" : "games.bundledDefinition")}
				</Badge>
				{game.verified ? (
					<span className="text-muted-foreground text-xs">{verifiedHint(t, game.verified)}</span>
				) : (
					game.origin === "user" && <span className="text-muted-foreground text-xs">{t("games.unverified")}</span>
				)}
			</dd>
			<dt className="text-muted-foreground">{t("settings.game.loader")}</dt>
			<dd className="flex flex-wrap items-center gap-1.5">
				{game.loader ? (
					<>
						<Badge variant="outline">{game.loader.name}</Badge>
						<code className="bg-muted rounded px-1.5 py-0.5 font-mono text-xs">{game.loader.id}</code>
					</>
				) : (
					<span className="text-muted-foreground text-sm">{t("settings.game.loaderNone")}</span>
				)}
			</dd>
			{game.loader ? (
				<>
					<dt className="text-muted-foreground">{t("settings.game.loaderVariant")}</dt>
					<dd className="flex flex-wrap items-center gap-1.5">
						{game.loader.variant ? (
							<>
								<Badge variant="secondary">{game.loader.variant}</Badge>
								{game.loader.default_variant ? <Badge variant="outline">{t("settings.game.loaderDefault")}</Badge> : null}
							</>
						) : (
							<span className="text-muted-foreground text-sm">{t("settings.game.loaderNone")}</span>
						)}
					</dd>
				</>
			) : null}
			<dt className="text-muted-foreground">
				{/* Steam is a product name (AGENTS §2.1.9), not translatable copy. */}
				{"Steam"}
			</dt>
			<dd className="flex flex-wrap gap-1.5">
				{game.steam_app_ids.map((appId) => (
					<Badge key={appId} variant="outline">
						{appId}
					</Badge>
				))}
			</dd>
			<dt className="text-muted-foreground">{t("settings.game.places")}</dt>
			<dd className="flex flex-wrap gap-1.5">
				{game.places.length === 0 ? (
					<span className="text-muted-foreground text-sm">{t("settings.game.fullRoot")}</span>
				) : (
					game.places.map((place) => (
						<code key={place} className="bg-muted rounded px-1.5 py-0.5 font-mono text-xs">
							{place}
						</code>
					))
				)}
			</dd>
		</dl>
	);
}

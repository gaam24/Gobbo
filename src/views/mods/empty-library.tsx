import { FolderOpen, Link2, Package, Upload } from "lucide-react";
import { useSettingsActions, useSettingsData } from "@/app/contexts/settings-context";
import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { useTranslation } from "@/i18n";

/// The list of a game with no mods yet: what to do first, in one centred block.
export function EmptyLibrary({
	gameName,
	libraryDir,
	onImport,
	onOpenFolder,
}: {
	gameName: string;
	libraryDir: string;
	onImport: () => void | Promise<void>;
	onOpenFolder: (path: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const { providers, sourceLinks } = useSettingsData();
	const { saveSourceLinks } = useSettingsActions();
	const linkProviders = providers.filter((provider) => provider.schemes.length > 0);
	const canTakeLinks = linkProviders.length > 0 && sourceLinks.length === 0;
	return (
		<Empty className="flex-1 border border-dashed p-10">
			<EmptyHeader>
				<EmptyMedia variant="icon" className="size-16 rounded-xl">
					<Package className="size-7" />
				</EmptyMedia>
				<EmptyTitle className="text-lg">{t("mods.empty.title", { name: gameName })}</EmptyTitle>
				<EmptyDescription>{t("mods.empty.body")}</EmptyDescription>
			</EmptyHeader>
			<EmptyContent>
				<div className="flex flex-wrap justify-center gap-2">
					<Button onClick={() => void onImport()}>
						<Upload /> {t("mods.import")}
					</Button>
					<Button variant="outline" disabled={!libraryDir} onClick={() => void onOpenFolder(libraryDir)}>
						<FolderOpen /> {t("mods.empty.openLibrary")}
					</Button>
				</div>
				{canTakeLinks && (
					<Empty className="border-border flex-row flex-wrap justify-between gap-3 border border-dashed p-4 text-left">
						<EmptyDescription className="max-w-md">{t("downloads.linksOffHint")}</EmptyDescription>
						<Button variant="outline" size="sm" onClick={() => void saveSourceLinks(linkProviders.map((provider) => provider.id))}>
							<Link2 /> {t("downloads.turnOn")}
						</Button>
					</Empty>
				)}
				<Empty className="border-border border border-dashed p-5">
					<EmptyDescription>{t("mods.empty.drop")}</EmptyDescription>
				</Empty>
			</EmptyContent>
		</Empty>
	);
}

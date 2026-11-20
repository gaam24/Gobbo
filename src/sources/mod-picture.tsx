import { cn } from "cn";
import { useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// Pictures read in this run, by provider, game, file and address: a new picture under the same file name reads again.
/// The cache is bounded: a data URL of a large picture stays in memory for as long as the map holds it. The oldest
/// entry leaves first; a visible `<img>` keeps its own reference to the URL it already shows.
const PICTURE_CACHE_MAX = 80;
const cache = new Map<string, Promise<string | null>>();

function pictureOf(key: string, gameId: string, provider: string, file: string): Promise<string | null> {
	let picture = cache.get(key);

	if (picture) {
		// Move a picture that is still in use to the end, so the oldest unused one leaves first.
		cache.delete(key);
		cache.set(key, picture);
	} else {
		picture = (async () => {
			try {
				return await unwrap(commands.sourcePicture(gameId, provider, file));
			} catch {
				// A picture that cannot be read is left out; the rest of the panel stands without it.
				return null;
			}
		})();

		if (cache.size >= PICTURE_CACHE_MAX) {
			const oldest = cache.keys().next().value;

			if (oldest !== undefined) cache.delete(oldest);
		}

		cache.set(key, picture);
	}

	return picture;
}

interface ModPictureProps {
	gameId: string;
	/// The provider whose page the picture came from.
	provider: string;
	/// The saved picture's file name, in the game's `images/` folder.
	file: string;
	/// The picture's address on the provider, to tell a new picture from the old one.
	url: string | null;
	name: string;
	/// The page is marked as adult content: the picture stays blurred until the user asks to see it.
	adult: boolean;
}

/// The saved picture of a provider page as a data URL: `undefined` while it loads, `null` when it cannot be read. A new
/// address under the same file name reads the picture again.
export function useModPicture(gameId: string, provider: string, file: string, url: string | null): string | null | undefined {
	const key = `${provider}/${gameId}/${file}/${url ?? ""}`;
	return useLoad(key, () => pictureOf(key, gameId, provider, file));
}

/// The main picture of the mod's provider page, at the top of the panel. A click shows it whole.
export function ModPicture({ gameId, provider, file, url, name, adult }: ModPictureProps) {
	const { t } = useTranslation();
	const picture = useModPicture(gameId, provider, file, url);
	const [revealed, setRevealed] = useState(false);
	const [enlarged, setEnlarged] = useState(false);
	const hidden = adult && !revealed;

	if (!picture) return null;
	return (
		<div className="bg-muted relative -mx-1 overflow-hidden rounded-lg border">
			<Button
				variant="ghost"
				disabled={hidden}
				aria-label={t("mods.picture.enlarge", { name })}
				className="h-auto w-full p-0 hover:bg-transparent"
				onClick={() => setEnlarged(true)}
			>
				<img src={picture} alt="" className={cn("aspect-video w-full object-cover", hidden && "scale-110 blur-2xl")} />
			</Button>
			{hidden && (
				<Button size="sm" variant="secondary" className="absolute inset-0 m-auto w-fit" onClick={() => setRevealed(true)}>
					{t("mods.picture.show")}
				</Button>
			)}
			<Dialog open={enlarged} onOpenChange={setEnlarged}>
				<DialogContent closeLabel={t("common.close")} className="grid max-w-[min(90vw,1200px)] gap-3 p-3 sm:max-w-[min(90vw,1200px)]">
					<DialogTitle className="px-1 text-sm">{name}</DialogTitle>
					<img src={picture} alt={t("mods.picture.alt", { name })} className="max-h-[80vh] w-full rounded-md object-contain" />
				</DialogContent>
			</Dialog>
		</div>
	);
}

import { cn } from "cn";
import { Check, CircleArrowUp, Puzzle, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { StatusBadge, type StatusTone } from "@/components/app/status-badge";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";
import { useSeen } from "@/lib/use-seen";
import { useModPicture } from "@/sources/mod-picture";

/// The state of a mod that the card shows on its picture: a missing requirement, a possible update, or a version that
/// is installed but not selected. Only one.
export interface CardSignal {
	kind: "missing" | "update" | "installed";
	text: string;
}

const SIGNAL_TONE: Record<CardSignal["kind"], StatusTone> = {
	missing: "danger",
	update: "update",
	installed: "neutral",
};

interface CardPictureProps {
	gameId: string;
	source: Mod["source"];
	enabled: boolean;
	signal: CardSignal | null;
}

/// The picture of a card, or a placeholder for a mod without one. The picture is read once the card first comes near the
/// view, so a long list does not read every picture at once. An adult picture stays blurred until the user asks for it.
export function CardPicture({ gameId, source, enabled, signal }: CardPictureProps) {
	const [ref, seen] = useSeen<HTMLDivElement>(Boolean(source?.picture));
	return (
		<div ref={ref} className={cn("bg-muted relative aspect-video overflow-hidden rounded-md", !enabled && "opacity-60")}>
			{source?.picture ? (
				seen && (
					<LoadedPicture
						gameId={gameId}
						provider={source.provider}
						file={source.picture}
						url={source.picture_url ?? null}
						adult={source.adult}
					/>
				)
			) : (
				<Placeholder />
			)}
			{signal && (
				<StatusBadge tone={SIGNAL_TONE[signal.kind]} className="bg-background/90 absolute end-1.5 top-1.5 max-w-[calc(100%-0.75rem)]">
					{signal.kind === "missing" && <TriangleAlert aria-hidden />}
					{signal.kind === "update" && <CircleArrowUp aria-hidden />}
					{signal.kind === "installed" && <Check aria-hidden />}
					<span className="truncate">{signal.text}</span>
				</StatusBadge>
			)}
		</div>
	);
}

/// The placeholder of a mod without a picture: a neutral box with a plain icon, no colour of its own.
function Placeholder() {
	return (
		<div className="text-subtle-foreground absolute inset-0 grid place-items-center">
			<Puzzle className="size-8" aria-hidden />
		</div>
	);
}

function LoadedPicture({
	gameId,
	provider,
	file,
	url,
	adult,
}: {
	gameId: string;
	provider: string;
	file: string;
	url: string | null;
	adult: boolean;
}) {
	const { t } = useTranslation();
	const picture = useModPicture(gameId, provider, file, url);
	const [revealed, setRevealed] = useState(false);
	// Still loading: the muted box stands in for the picture.
	if (picture === undefined) return null;

	if (picture === null) return <Placeholder />;
	const hidden = adult && !revealed;
	return (
		<>
			<img src={picture} alt="" className={cn("absolute inset-0 size-full object-cover", hidden && "scale-110 blur-2xl")} />
			{hidden && (
				<Button size="sm" variant="secondary" className="absolute inset-0 m-auto w-fit" onClick={() => setRevealed(true)}>
					{t("mods.picture.show")}
				</Button>
			)}
		</>
	);
}

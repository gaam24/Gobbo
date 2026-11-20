import { ArrowUpCircle, Boxes, Plus, RefreshCw } from "lucide-react";

import { useCollections } from "@/app/contexts/collections-context";
import type { ProfileView } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { collectionKey, hasCollectionUpdate, revisionLabel } from "@/lib/collections";

/// The active profile's collections in the header. A collection opens its panel; the menu can start a new one.
export function CollectionBadge({ profile }: { profile: ProfileView | undefined }) {
	const { t } = useTranslation();
	const { openPanel, openLink, installs, checkUpdates, checking } = useCollections();
	const collections = profile?.collections ?? [];

	if (collections.length === 0) return null;
	const active = [...installs.values()].some(
		(install) =>
			!install.stopped &&
			install.identity.profile === profile?.id &&
			collections.some((origin) => collectionKey(origin) === install.identity.collection),
	);
	const first = collections[0];
	const label = collections.length === 1 && first ? first.name : t("collection.badge.count", { count: collections.length });
	const hasUpdate = hasCollectionUpdate(collections);
	const details = collections
		.map((item) => `${item.name} · ${revisionLabel(item)}${item.update ? ` · ${t("collection.badge.update")}` : ""}`)
		.join("\n");
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="outline" size="sm" className="max-w-56" title={details}>
					<Boxes />
					{hasUpdate && <ArrowUpCircle className="text-update size-3.5 shrink-0" aria-label={t("collection.badge.update")} />}
					<span className="min-w-0 truncate">{label}</span>
					{active && <Spinner className="size-3.5 shrink-0" aria-label={t("tasks.collectionInstalling", { name: label })} />}
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-72">
				<DropdownMenuLabel>{t("collection.badge.title")}</DropdownMenuLabel>
				{collections.map((origin) => (
					<DropdownMenuItem
						key={`${origin.provider}:${origin.game_key}:${origin.slug}`}
						onSelect={() => {
							if (!profile) return;
							void openPanel(profile.id, collectionKey(origin));
						}}
					>
						<Boxes />
						<span className="min-w-0 flex-1 truncate">{origin.name}</span>
						{origin.update && <ArrowUpCircle className="text-update size-3.5 shrink-0" aria-label={t("collection.badge.update")} />}
						<span className={`font-mono text-xs tabular-nums ${origin.update ? "text-update font-medium" : "text-subtle-foreground"}`}>
							{revisionLabel(origin)}
						</span>
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuItem disabled={checking} onSelect={() => void (profile ? checkUpdates(profile.id) : undefined)}>
					<RefreshCw className={checking ? "animate-spin" : undefined} /> {t("collection.badge.check")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={openLink}>
					<Plus /> {t("collection.installAction")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

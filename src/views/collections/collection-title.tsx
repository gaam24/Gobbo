import type { ReactNode } from "react";

import { useSettingsData } from "@/app/contexts/settings-context";
import { Badge } from "@/components/ui/badge";
import { useTranslation } from "@/i18n";
import { providerName } from "@/lib/collections";
import { formatSize } from "@/lib/mods";

/// The title of a collection screen: the name, the revision, the provider and the badges the screen adds.
export function CollectionTitle({
	name,
	revision,
	provider,
	adult,
	children,
}: {
	name: string;
	revision: string;
	provider: string;
	adult?: boolean;
	children?: ReactNode;
}) {
	const { t } = useTranslation();
	const { providers } = useSettingsData();
	return (
		<span className="flex min-w-0 flex-wrap items-center gap-2 pr-6">
			<span className="min-w-0 truncate">{name}</span>
			<Badge variant="secondary" className="font-mono">
				{revision}
			</Badge>
			<Badge variant="outline">{providerName(providers, provider)}</Badge>
			{adult && <Badge variant="destructive">{t("collection.adult")}</Badge>}
			{children}
		</span>
	);
}

/// The meta line under a collection title: the author, the member count, the size when the provider knows it, and
/// anything the screen adds (the account).
export function collectionMeta(
	t: ReturnType<typeof useTranslation>["t"],
	language: string,
	info: { author?: string | null; mod_count: number; total_size_kb: number },
	extra: Array<string | null | undefined> = [],
): string {
	return [
		info.author,
		t("collection.members", { count: info.mod_count }),
		info.total_size_kb > 0 ? formatSize(info.total_size_kb, language) : null,
		...extra,
	]
		.filter(Boolean)
		.join(" · ");
}

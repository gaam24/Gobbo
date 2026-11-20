import { CircleArrowUp, X } from "lucide-react";
import type { ImportedModView } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";

/// A manual import added a version of a mod that already had a selected one. Switching is the user's choice: the
/// banner names the mod and the new version, and never changes the list by itself.
export function ImportVersionBanner({
	mods,
	onUse,
	onDismiss,
}: {
	mods: ImportedModView[];
	onUse: (item: ImportedModView) => void | Promise<void>;
	onDismiss: (item: ImportedModView) => void;
}) {
	const { t } = useTranslation();

	if (mods.length === 0) return null;

	return (
		<div role="status" className="bg-muted/60 flex shrink-0 flex-col gap-2 border-b px-6 py-2.5 text-sm">
			<div className="text-muted-foreground flex items-center gap-2">
				<CircleArrowUp className="size-4 shrink-0" aria-hidden />
				<span>{t("mods.importVersion.title")}</span>
			</div>
			<ul className="grid gap-1.5">
				{mods.map((item) => (
					<li key={item.folder} className="flex min-w-0 items-center gap-2 pl-6">
						<span className="min-w-0 truncate font-medium">
							{item.name}
							<span className="text-muted-foreground font-normal">
								{" · "}
								{item.version ?? item.folder}
							</span>
						</span>
						<Button size="sm" variant="outline" className="ml-auto" onClick={() => void onUse(item)}>
							{t("mods.importVersion.use")}
						</Button>
						<Button
							size="icon-xs"
							variant="ghost"
							aria-label={t("mods.importVersion.keep", { name: item.name })}
							title={t("mods.importVersion.keep", { name: item.name })}
							onClick={() => onDismiss(item)}
						>
							<X aria-hidden />
						</Button>
					</li>
				))}
			</ul>
		</div>
	);
}

import { ArrowLeft } from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";
import { commands } from "@/bindings";
import { StatusLine } from "@/components/app/content-blocks";
import { PathText } from "@/components/app/path-text";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";

/// One copy of a conflicting file. The family picker switches between the winner and the overridden copies, so a
/// texture can be compared without leaving the dialog. The bytes come from a command only when a copy is shown.
export function ConflictPreviewPanel({
	gameId,
	path,
	families,
	onBack,
}: {
	gameId: string;
	path: string;
	families: Array<{ id: string; name: string }>;
	onBack: () => void;
}) {
	const { t, i18n } = useTranslation();
	const [family, setFamily] = useState(families[0]?.id ?? "");
	const preview = useLoad(`conflict-preview:${gameId}:${path}:${family}`, () => unwrap(commands.conflictFilePreview(gameId, path, family)));
	let previewBody: ReactNode;

	if (!preview) {
		previewBody = <StatusLine tone="loading">{t("conflicts.loading")}</StatusLine>;
	} else if (preview.data_url) {
		previewBody = (
			<img
				src={preview.data_url}
				alt={t("conflicts.previewAlt", { path })}
				className="mx-auto max-h-[55vh] max-w-full rounded-lg border object-contain"
			/>
		);
	} else {
		previewBody = (
			<p className="text-muted-foreground text-sm">
				{t("conflicts.unsupported", {
					extension: preview.extension || "?",
					size: formatSize(Math.ceil(preview.bytes / 1024), i18n.language),
				})}
			</p>
		);
	}

	return (
		<div className="grid gap-3">
			<div className="flex flex-wrap items-center gap-2">
				<Button variant="ghost" size="sm" onClick={onBack}>
					<ArrowLeft /> {t("conflicts.back")}
				</Button>
				<PathText path={path} className="text-muted-foreground flex-1" />
				{families.length > 1 && (
					<Select value={family} onValueChange={setFamily}>
						<SelectTrigger className="h-8 w-48 text-xs" aria-label={t("conflicts.copy")}>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{families.map((entry) => (
								<SelectItem key={entry.id} value={entry.id}>
									{entry.name}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				)}
			</div>
			{previewBody}
		</div>
	);
}

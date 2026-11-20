import { FileArchive } from "lucide-react";
import { useState } from "react";
import type { DefinitionPreview } from "@/bindings";
import { commands } from "@/bindings";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { ARCHIVE_EXTENSIONS } from "@/lib/archives";
import { messageOf, unwrap } from "@/lib/ipc";
import { pickFiles } from "@/lib/tauri";

/// A dry run against a real archive. The backend uses the same placement rules as an import and writes nothing, so a
/// user can see whether a new definition understands the mods they have before saving it.
export function ArchiveCheck({ text }: { text: string }) {
	const { t } = useTranslation();
	const [preview, setPreview] = useState<DefinitionPreview | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const [file, setFile] = useState<string | null>(null);

	const run = async () => {
		const [picked] = await pickFiles(t("editor.archive.filter"), ARCHIVE_EXTENSIONS, false);

		if (!picked) return;
		setFile(picked);
		setBusy(true);
		setError(null);

		try {
			setPreview(await unwrap(commands.previewDefinition(text, picked)));
		} catch (failure) {
			setPreview(null);
			setError(messageOf(failure));
		}

		setBusy(false);
	};

	return (
		<section className="grid gap-3 rounded-lg border p-4">
			<div className="flex flex-wrap items-center justify-between gap-3">
				<div className="grid gap-1">
					<h3 className="text-sm font-semibold">{t("editor.archive.title")}</h3>
					<p className="text-muted-foreground text-xs">{t("editor.archive.hint")}</p>
				</div>
				<Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => void run()}>
					{busy ? <Spinner aria-hidden /> : <FileArchive />}
					{t("editor.archive.choose")}
				</Button>
			</div>
			{file && <p className="text-muted-foreground truncate font-mono text-xs">{file}</p>}
			{error && (
				<Alert variant="destructive">
					<AlertDescription>{error}</AlertDescription>
				</Alert>
			)}
			{preview && (
				<div className="grid gap-3">
					<div className="flex flex-wrap items-center gap-2 text-sm">
						<Badge variant="secondary">{t("editor.archive.mods", { count: preview.mods.length })}</Badge>
						<Badge variant={preview.skipped > 0 ? "outline" : "secondary"}>{t("editor.archive.skipped", { count: preview.skipped })}</Badge>
					</div>
					<ul className="grid gap-3">
						{preview.mods.map((mod) => (
							<li key={mod.name} className="grid gap-1.5">
								<p className="text-sm font-medium">
									{mod.name}
									<span className="text-muted-foreground font-normal">
										{" · "}
										{mod.slot ? t("editor.archive.inSlot", { slot: mod.slot }) : t("editor.archive.gamePaths")}
										{" · "}
										{t("editor.archive.files", { count: mod.files })}
									</span>
								</p>
								<ul className="text-muted-foreground grid gap-0.5 font-mono text-xs">
									{mod.examples.map((example) => (
										<li key={`${example.from}-${example.to}`} className="truncate">
											{example.from}
											{" → "}
											{example.to}
										</li>
									))}
								</ul>
							</li>
						))}
					</ul>
					{preview.skipped_examples.length > 0 && (
						<p className="text-muted-foreground text-xs">
							{t("editor.archive.skippedExamples", { files: preview.skipped_examples.join(", ") })}
						</p>
					)}
				</div>
			)}
		</section>
	);
}

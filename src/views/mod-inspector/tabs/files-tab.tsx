import { cn } from "cn";
import { FileSearch, ShieldCheck } from "lucide-react";
import type { IntegrityStatus, ModDetails as ModDetailsData } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { PathText } from "@/components/app/path-text";
import { StatusBadge, type StatusTone } from "@/components/app/status-badge";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { ItemContent } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { translate } from "@/lib/ipc";
import { FILE_STATE } from "@/lib/mods";

/// The files of the mod, each with the state it has in the game. The dot and the message are the whole status: the
/// message already says "No conflict", "Mod disabled" or "Wins, overrides: …", so no second label repeats it.
const INTEGRITY_TONE: Record<IntegrityStatus, StatusTone> = {
	Matches: "success",
	Changed: "danger",
	Recorded: "info",
};

export function FilesTab({
	files,
	count,
	skipped,
	integrity,
	verifying,
	onVerify,
	onArchiveReport,
}: {
	/// `null` while the files of the mod load.
	files: ModDetailsData["files"] | null;
	count: number;
	/// Entries the import could not place.
	skipped: number;
	integrity: IntegrityStatus | null;
	verifying: boolean;
	onVerify: () => void;
	onArchiveReport: () => void;
}) {
	const { t } = useTranslation();
	const integrityText = {
		Matches: t("mods.integrity.matches"),
		Changed: t("mods.integrity.changed"),
		Recorded: t("mods.integrity.recorded"),
	}[integrity ?? "Recorded"];

	if (files === null) return <p className="text-muted-foreground text-sm">{t("mods.files.loading")}</p>;

	if (files.length === 0) {
		return (
			<Empty className="border border-dashed">
				<EmptyHeader>
					<EmptyDescription>{t("mods.files.empty")}</EmptyDescription>
				</EmptyHeader>
			</Empty>
		);
	}

	return (
		<div className="grid gap-2">
			{skipped > 0 && (
				<div className="bg-muted/40 flex flex-wrap items-center gap-2 rounded-lg border p-2 text-sm">
					<span className="min-w-0 flex-1">{t("mods.files.skipped", { count: skipped })}</span>
					<Button variant="outline" size="sm" onClick={onArchiveReport}>
						<FileSearch /> {t("mods.archiveReport")}
					</Button>
				</div>
			)}
			<p className="text-muted-foreground text-xs tabular-nums">{t("mods.files.count", { count })}</p>
			<div className="bg-muted/40 flex flex-wrap items-center gap-2 rounded-lg border p-2 text-sm">
				<Button variant="outline" size="sm" disabled={verifying} onClick={onVerify}>
					<ShieldCheck /> {verifying ? t("mods.integrity.checking") : t("mods.integrity.verify")}
				</Button>
				{integrity && (
					<StatusBadge tone={INTEGRITY_TONE[integrity]} dot>
						{integrityText}
					</StatusBadge>
				)}
			</div>
			<BoxList scroll tall>
				{files.map((file) => (
					<BoxListRow key={file.path} className="py-2">
						<ItemContent className="gap-0.5">
							<PathText path={file.path} />
							<span className="flex min-w-0 items-center gap-2 text-xs">
								<span aria-hidden className={cn("size-2 shrink-0 rounded-full", FILE_STATE[file.state].dot)} />
								<span className={cn("min-w-0 truncate", FILE_STATE[file.state].text)}>{translate(file.result)}</span>
							</span>
						</ItemContent>
					</BoxListRow>
				))}
			</BoxList>
		</div>
	);
}

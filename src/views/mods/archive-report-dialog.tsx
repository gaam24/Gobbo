import { CircleAlert } from "lucide-react";
import { type ReactNode, useState } from "react";
import { commands, type SkipReason } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { PathText } from "@/components/app/path-text";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { FieldError } from "@/components/ui/field";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// What the archive holds against the installed folder: what the import left out, what is missing now, what appeared
/// later and what has another size. The report is read-only; it never changes the library.
export function ArchiveReportDialog({
	gameId,
	folder,
	name,
	onClose,
}: {
	gameId: string;
	folder: string;
	name: string;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	const report = useLoad(`${gameId}:${folder}`, () => unwrap(commands.archiveReport(gameId, folder)), {
		onError: (error) => setFailure(messageOf(error)),
	});

	const reason = (value: SkipReason) => t(`archiveReport.reason.${value}`);
	const skipped = report?.skipped.filter((file) => file.reason === "no_rule") ?? [];
	const otherSkipped = report?.skipped.filter((file) => file.reason !== "no_rule") ?? [];

	let body: ReactNode;

	if (failure) {
		body = (
			<FieldError className="flex items-start gap-2">
				<CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
				{failure}
			</FieldError>
		);
	} else if (!report) {
		body = <p className="text-muted-foreground text-sm">{t("archiveReport.loading")}</p>;
	} else {
		const clean = report.missing.length === 0 && report.extra.length === 0 && report.changed.length === 0 && report.skipped.length === 0;
		body = (
			<>
				<p className="text-muted-foreground">
					{t("archiveReport.summary", {
						archive: report.archive,
						expected: report.expected,
						installed: report.installed,
					})}
				</p>
				{clean && <p>{t("archiveReport.matches")}</p>}
				<FileList title={t("archiveReport.skipped", { count: report.skipped_total })} files={skipped.map((file) => file.path)} />
				<FileList title={t("archiveReport.missing", { count: report.missing.length })} files={report.missing} />
				<FileList title={t("archiveReport.extra", { count: report.extra.length })} files={report.extra} />
				<FileList title={t("archiveReport.changed", { count: report.changed.length })} files={report.changed} />
				{otherSkipped.length > 0 && (
					<FileList
						title={t("archiveReport.otherSkipped", { count: otherSkipped.length })}
						files={otherSkipped.map((file) => `${file.path} — ${reason(file.reason)}`)}
					/>
				)}
			</>
		);
	}

	return (
		<ModalDialog title={t("archiveReport.title", { name })} description={t("archiveReport.hint")} width="lg" onClose={onClose}>
			{body}
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

// A titled, scrollable list of paths. An empty list draws nothing, so the dialog stays short when everything matches.
function FileList({ title, files }: { title: string; files: string[] }) {
	if (files.length === 0) return null;
	return (
		<section className="grid gap-2">
			<SectionHeading title={title} />
			<BoxList scroll>
				{files.map((path) => (
					<BoxListRow key={path}>
						<ItemContent className="min-w-0">
							<ItemTitle className="w-full font-normal">
								<PathText path={path} />
							</ItemTitle>
						</ItemContent>
					</BoxListRow>
				))}
			</BoxList>
		</section>
	);
}

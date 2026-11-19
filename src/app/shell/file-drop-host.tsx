import { useNoticeActions } from "@/app/contexts/notices-context";
import { useUiActions } from "@/app/contexts/ui-context";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { commands } from "@/bindings";
import { useTranslation } from "@/i18n";
import { CommandError, messageOf, unwrap } from "@/lib/ipc";
import { useFileDrop } from "@/lib/use-file-drop";

const ARCHIVE_EXTENSIONS = [".zip", ".7z", ".rar"];

/// The full-window hint shown while files are dragged over the app. A single archive is checked for a definition
/// package first; if it is one, the import dialog opens. Anything else is a mod archive and follows the usual import.
export function FileDropHost() {
	const { t } = useTranslation();
	const { importPaths } = useWorkspaceActions();
	const { openDialog } = useUiActions();
	const { notifyError } = useNoticeActions();
	const dragging = useFileDrop(async (paths) => {
		const path = paths[0];
		if (paths.length === 1 && path !== undefined && isArchive(path)) {
			try {
				await unwrap(commands.inspectDefinitionPackage(path));
				openDialog({ kind: "definition-import", path });
				return;
			} catch (failure) {
				const notADefinition = failure instanceof CommandError && failure.appError.code === "games.package_empty";
				if (!notADefinition) {
					notifyError(messageOf(failure), failure instanceof Error ? failure.message : undefined);
					return;
				}
			}
		}
		await importPaths(paths);
	});

	if (!dragging) return null;

	return (
		<div className="border-foreground bg-background/85 pointer-events-none fixed inset-0 z-50 m-3 flex items-center justify-center rounded-lg border-2 border-dashed text-lg font-medium">
			{t("import.dropHint")}
		</div>
	);
}

function isArchive(path: string): boolean {
	const lower = path.toLowerCase();
	return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

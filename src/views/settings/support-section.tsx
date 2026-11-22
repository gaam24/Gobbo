import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { copyText, openPath } from "@/lib/open";
import { SectionBlock } from "@/views/settings/controls";
import { useSupportBundle } from "@/views/settings/use-support-bundle";

/// The support bundle is read-only: the log, the version and, for the current game, its definition, setup check and
/// last run. Paths are redacted by the backend; credentials never enter the bundle.
export function SupportSection({ gameId }: { gameId: string | null }) {
	const { t } = useTranslation();
	const { path, busy, failure, setFailure, create } = useSupportBundle(gameId);

	return (
		<SectionBlock>
			<div className="grid gap-3">
				<div className="flex flex-wrap items-center gap-2">
					<Button variant="outline" size="sm" disabled={busy} onClick={() => void create()}>
						{busy ? t("settings.support.creating") : t("settings.support.create")}
					</Button>
					{path && (
						<>
							<Button variant="ghost" size="sm" onClick={() => void openPath(path, gameId, true, setFailure)}>
								{t("settings.support.open")}
							</Button>
							<Button variant="ghost" size="sm" onClick={() => void copyText(path, setFailure)}>
								{t("settings.support.copy")}
							</Button>
						</>
					)}
				</div>
				{path && <p className="text-muted-foreground font-mono text-xs break-all">{path}</p>}
				{failure && <FieldError>{t("settings.support.failed", { detail: failure })}</FieldError>}
			</div>
		</SectionBlock>
	);
}

import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import { useTranslation } from "@/i18n";
import { SectionBlock } from "@/views/settings/controls";
import { useAppUpdates } from "@/views/settings/use-app-updates";

/// The update channel of this copy: the current version, the check and the install of a newer one.
export function UpdatesSection() {
	const { t } = useTranslation();
	const { version, channel, state, checkNow } = useAppUpdates();
	const unsupported = channel === "unsupported";

	return (
		<SectionBlock>
			<div className="grid gap-3">
				<p className="text-muted-foreground text-sm">
					{version ? t("settings.updates.current", { version }) : t("settings.updates.versionUnknown")}
				</p>
				{channel === "portable" && <p className="text-muted-foreground text-sm">{t("settings.updates.portableNote")}</p>}
				{unsupported && <p className="text-muted-foreground text-sm">{t("settings.updates.unsupported")}</p>}
				{state.kind === "available" && <p className="text-sm">{t("settings.updates.available", { version: state.version })}</p>}
				{state.kind === "current" && <p className="text-muted-foreground text-sm">{t("settings.updates.currentLatest")}</p>}
				{state.kind === "failed" && <FieldError>{t("settings.updates.failed", { detail: state.detail })}</FieldError>}
				{state.kind === "installing" && (
					<>
						<Progress
							value={state.applying ? undefined : state.percent}
							aria-label={t("settings.updates.installing")}
							aria-valuetext={state.percent === undefined || state.applying ? undefined : `${state.percent}%`}
						/>
						{state.applying && <p className="text-muted-foreground text-sm">{t("settings.updates.applying")}</p>}
					</>
				)}
				<div className="flex flex-wrap gap-2">
					{!unsupported && (
						<Button
							variant="outline"
							size="sm"
							disabled={state.kind === "checking" || state.kind === "installing"}
							onClick={() => void checkNow()}
						>
							{state.kind === "checking" ? t("settings.updates.checking") : t("settings.updates.check")}
						</Button>
					)}
					{state.kind === "available" && (
						<Button size="sm" onClick={state.install}>
							{t("settings.updates.install")}
						</Button>
					)}
				</div>
			</div>
		</SectionBlock>
	);
}

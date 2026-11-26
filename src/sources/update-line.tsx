import { cn } from "cn";
import { CircleArrowUp, ExternalLink, Eye, EyeOff } from "lucide-react";
import type { ReactNode } from "react";
import type { UpdateReport } from "@/bindings";
import { StatusLine } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { errorText } from "@/lib/ipc";

/// The version state of a mod, as text with an icon. An update is never shown by colour alone. With `onOpen`, the
/// version is a button that opens the files page of the mod; with `onIgnore`, the version can be hidden until the
/// provider lists another one.
export function UpdateLine({
	update,
	onOpen,
	onUse,
	onIgnore,
	onUnignore,
}: {
	update?: UpdateReport;
	onOpen?: () => void;
	/// Switches the profile to the newer version that is already installed.
	onUse?: () => void;
	onIgnore?: () => void;
	onUnignore?: () => void;
}) {
	const { t } = useTranslation();

	if (!update) return null;

	if (update.error) {
		return <StatusLine tone="error">{errorText(update.error)}</StatusLine>;
	}

	if (update.state === "ignored") {
		return (
			<div className="grid gap-1">
				<p className="text-subtle-foreground flex items-center gap-1.5 text-sm">
					<EyeOff className="size-3.5 shrink-0" aria-hidden />
					{update.latest ? t("mods.version.ignored", { version: update.latest }) : t("mods.version.ignoredShort")}
				</p>
				{onUnignore && (
					<IgnoreButton icon={<Eye className="size-3.5" aria-hidden />} label={t("mods.version.showIgnored")} onClick={onUnignore} />
				)}
			</div>
		);
	}

	if (update.has_update) {
		if (!update.latest) return <p className="text-muted-foreground text-sm">{t("mods.version.upToDate")}</p>;
		const installed = update.latest_installed;
		const label = installed
			? t("mods.version.installed", { version: update.latest })
			: t("mods.version.updateAvailable", { version: update.latest });
		const content = (
			<>
				<CircleArrowUp className="size-3.5 shrink-0" aria-hidden />
				<span className="truncate">{label}</span>
				{onOpen && !installed && <ExternalLink className="size-3 shrink-0 opacity-70" aria-hidden />}
			</>
		);
		let line: ReactNode;

		if (onOpen && !installed) {
			line = (
				<Button
					variant="link"
					title={t("mods.version.openUpdate")}
					className={cn("text-update", "h-auto justify-start gap-1.5 p-0 text-sm font-medium")}
					onClick={onOpen}
				>
					{content}
				</Button>
			);
		} else if (installed && onUse) {
			line = (
				<Button
					variant="link"
					title={t("mods.version.useInstalled", { version: update.latest })}
					aria-label={t("mods.version.useInstalled", {
						version: update.latest,
					})}
					className="text-muted-foreground h-auto justify-start gap-1.5 p-0 text-sm font-medium"
					onClick={onUse}
				>
					{content}
				</Button>
			);
		} else {
			line = <p className="text-update flex items-center gap-1.5 text-sm font-medium">{content}</p>;
		}

		return (
			<div className="grid gap-1">
				{line}
				{onIgnore && (
					<IgnoreButton icon={<EyeOff className="size-3.5" aria-hidden />} label={t("mods.version.ignore")} onClick={onIgnore} />
				)}
			</div>
		);
	}

	if (update.state === "current") return <p className="text-muted-foreground text-sm">{t("mods.version.upToDate")}</p>;
	return <p className="text-muted-foreground text-sm">{t("mods.version.notChecked")}</p>;
}

function IgnoreButton({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
	return (
		<Button variant="ghost" size="sm" className="text-muted-foreground h-auto justify-self-start px-1.5 py-0.5 text-xs" onClick={onClick}>
			{icon}
			{label}
		</Button>
	);
}

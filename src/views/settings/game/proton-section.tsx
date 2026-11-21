import { FolderOpen, RefreshCw, RotateCcw } from "lucide-react";
import type { ProtonBuild, Status } from "@/bindings";
import { PathText } from "@/components/app/path-text";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { usePlatform } from "@/lib/platform";
import { Section, SettingRow, SettingsGroup } from "@/views/settings/controls";
import { PROTON_STEAM, protonAutomaticLabel, protonChoices } from "@/views/settings/game/proton-choices";
import { PROTON_FPS_LIMITS, PROTON_OPTION_GROUPS, type ProtonOption } from "@/views/settings/game/proton-options";

/// Everything Proton does for the game: the build it starts with, the prefix of an install that is not from Steam, and
/// the switches for its runtime. All act at once. Proton runs games on Linux only, so the section is not shown anywhere
/// else.
export function ProtonSection({
	status,
	protonBuilds,
	protonDir,
	refreshing,
	onPickBuild,
	onPickFolder,
	onRefreshBuilds,
	prefixDir,
	onPickPrefix,
	onResetPrefix,
	offered,
	onSwitch,
	fpsLimit,
	onFpsLimit,
}: {
	status: Status;
	protonBuilds: ProtonBuild[] | undefined;
	protonDir: string;
	refreshing: boolean;
	onPickBuild: (folder: string) => void | Promise<void>;
	onPickFolder: () => void | Promise<void>;
	onRefreshBuilds: () => void | Promise<void>;
	/// The chosen Wine prefix of an install that is not from Steam; empty for the one Gobbo owns.
	prefixDir: string;
	onPickPrefix: () => void | Promise<void>;
	onResetPrefix: () => void | Promise<void>;
	offered: readonly ProtonOption[];
	onSwitch: (option: ProtonOption, on: boolean) => void | Promise<void>;
	fpsLimit: number | null;
	onFpsLimit: (limit: number | null) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const platform = usePlatform();

	if (platform !== "linux") return null;
	const chosen = status.settings.proton_options ?? [];
	// The backend can offer an id this build does not label; hide its group when nothing is left to show.
	const groups = PROTON_OPTION_GROUPS.map((group) => ({
		id: group.id,
		options: group.options.filter((option) => offered.includes(option)),
	})).filter((group) => group.options.length > 0);

	return (
		<Section id="settings-proton" title={t("settings.proton.title")} hint={t("settings.proton.hint")}>
			<SettingRow
				title={t("settings.proton.version")}
				htmlFor="proton-build"
				hint={protonBuilds?.length === 0 ? t("settings.proton.none") : undefined}
				help={t("settings.proton.help")}
			>
				<div className="flex gap-2">
					<Select value={protonDir || PROTON_STEAM} onValueChange={(value) => void onPickBuild(value === PROTON_STEAM ? "" : value)}>
						<SelectTrigger id="proton-build" className="min-w-0 flex-1">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value={PROTON_STEAM}>{t(protonAutomaticLabel(status.runtime))}</SelectItem>
							{protonChoices(protonBuilds ?? [], protonDir).map((build) => (
								<SelectItem key={build.folder} value={build.folder}>
									{build.name}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<Button
						type="button"
						variant="outline"
						size="icon"
						aria-label={t("settings.proton.refresh")}
						title={t("settings.proton.refresh")}
						onClick={() => void onRefreshBuilds()}
						disabled={refreshing}
					>
						{refreshing ? <Spinner aria-hidden /> : <RefreshCw />}
					</Button>
					<Button type="button" variant="outline" className="shrink-0" onClick={() => void onPickFolder()}>
						<FolderOpen /> {t("settings.proton.choose")}
					</Button>
				</div>
			</SettingRow>

			{status.runtime === "owned_proton" && (
				<SettingRow title={t("settings.proton.prefix")} hint={t("settings.proton.prefixHint")} help={t("settings.proton.prefixHelp")}>
					<div className="flex min-w-0 items-center gap-2">
						{prefixDir ? (
							<PathText path={prefixDir} className="text-muted-foreground min-w-0 flex-1" />
						) : (
							<span className="text-muted-foreground min-w-0 flex-1 truncate text-sm">{t("settings.proton.prefixOwned")}</span>
						)}
						{prefixDir && (
							<Button
								type="button"
								variant="outline"
								size="icon"
								aria-label={t("settings.proton.prefixReset")}
								title={t("settings.proton.prefixReset")}
								onClick={() => void onResetPrefix()}
							>
								<RotateCcw />
							</Button>
						)}
						<Button type="button" variant="outline" className="shrink-0" onClick={() => void onPickPrefix()}>
							<FolderOpen /> {t("settings.proton.prefixChoose")}
						</Button>
					</div>
				</SettingRow>
			)}

			<SettingRow
				title={t("settings.proton.fpsLimit")}
				htmlFor="proton-fps"
				hint={t("settings.proton.fpsHint")}
				help={t("settings.proton.fpsHelp")}
			>
				<Select
					value={fpsLimit === null ? "off" : String(fpsLimit)}
					onValueChange={(value) => void onFpsLimit(value === "off" ? null : Number(value))}
				>
					<SelectTrigger id="proton-fps" className="min-w-0 flex-1">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value="off">{t("settings.proton.unlimited")}</SelectItem>
						{PROTON_FPS_LIMITS.map((fps) => (
							<SelectItem key={fps} value={String(fps)}>
								{fps}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</SettingRow>

			{groups.map((group, index) => (
				<SettingsGroup key={group.id} title={t(`settings.protonOptions.groups.${group.id}`)} className={index === 0 ? undefined : "mt-10"}>
					{group.options.map((option) => (
						<SettingRow
							key={option}
							title={t(`settings.protonOptions.items.${option}.title`)}
							htmlFor={`proton-${option}`}
							hint={t(`settings.protonOptions.items.${option}.hint`)}
							help={t(`settings.protonOptions.items.${option}.help`)}
						>
							<Switch
								id={`proton-${option}`}
								aria-label={t(`settings.protonOptions.items.${option}.title`)}
								checked={chosen.includes(option)}
								onCheckedChange={(on) => void onSwitch(option, on)}
							/>
						</SettingRow>
					))}
				</SettingsGroup>
			))}
		</Section>
	);
}

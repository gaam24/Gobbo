import type { Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import { commitOnEnter, Section, SettingRow } from "@/views/settings/controls";

/// What the game is started with: its parameters and environment variables, and the Steam launch option when the game
/// is started from Steam. The Steam line is copied as one string because Steam's field is outside Gobbo.
export function StartSection({
	status,
	launchEnv,
	setLaunchEnv,
	launchArgs,
	setLaunchArgs,
	onCopy,
	onCommit,
}: {
	status: Status;
	launchEnv: string;
	setLaunchEnv: (value: string) => void;
	launchArgs: string;
	setLaunchArgs: (value: string) => void;
	onCopy: (text: string) => void | Promise<void>;
	/// Saves the typed start options when a field loses focus or Enter is pressed in a single-line field.
	onCommit: () => void;
}) {
	const { t } = useTranslation();

	return (
		<Section id="settings-start" title={t("settings.start.title")} hint={t("settings.start.hint")}>
			<SettingRow title={t("settings.args.title")} htmlFor="launch-args" hint={t("settings.args.hint")}>
				<Input
					id="launch-args"
					value={launchArgs}
					onChange={(event) => setLaunchArgs(event.currentTarget.value)}
					onBlur={onCommit}
					onKeyDown={commitOnEnter}
					placeholder={t("settings.args.placeholder")}
					spellCheck={false}
				/>
			</SettingRow>

			<SettingRow title={t("settings.env.title")} htmlFor="launch-env" hint={t("settings.env.hint")} help={t("settings.env.help")}>
				<Textarea
					id="launch-env"
					value={launchEnv}
					onChange={(event) => setLaunchEnv(event.currentTarget.value)}
					onBlur={onCommit}
					placeholder="DXVK_HUD=fps"
					rows={3}
					spellCheck={false}
					className="font-mono text-xs"
				/>
			</SettingRow>

			{status.launch_hint && (
				<SettingRow title={t("deploy.steamOption.title")} hint={t("deploy.steamOption.body")}>
					<div className="grid gap-3">
						<ol className="text-muted-foreground grid list-decimal gap-1 pl-5 text-sm">
							<li>{t("settings.steam.step1")}</li>
							<li>{t("settings.steam.step2")}</li>
							<li>{t("settings.steam.step3")}</li>
						</ol>
						<code className="bg-muted block rounded-lg border p-2.5 font-mono text-xs break-all">{status.launch_hint}</code>
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="justify-self-start"
							onClick={() => void onCopy(status.launch_hint)}
						>
							{t("settings.steam.copy")}
						</Button>
					</div>
				</SettingRow>
			)}
		</Section>
	);
}

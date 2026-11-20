import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { commands, type PortableUpdateStep, type UpdateChannel } from "@/bindings";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Progress } from "@/components/ui/progress";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { newProgressChannel } from "@/lib/tauri";
import { SectionBlock } from "@/views/settings/controls";

type State =
	| { kind: "idle" }
	| { kind: "checking" }
	| { kind: "current" }
	| { kind: "available"; version: string; install: () => void }
	| { kind: "installing"; percent?: number; applying: boolean }
	| { kind: "failed"; detail: string };

/// The update channel. An installed copy checks the manifest through the updater plugin and runs the installer. A
/// portable copy checks the same manifest with its own key and replaces its program files itself (`update/`). The
/// endpoint and the public key are in `tauri.conf.json`; the release workflow signs both kinds with the matching key.
export function UpdatesSection() {
	const { t } = useTranslation();
	const [version, setVersion] = useState<string | null>(null);
	const [channel, setChannel] = useState<UpdateChannel>("installed");
	const [state, setState] = useState<State>({ kind: "idle" });

	useEffect(() => {
		const load = async () => {
			try {
				setVersion(await getVersion());
			} catch {
				setVersion(null);
			}

			try {
				setChannel(await unwrap(commands.updateChannel()));
			} catch {
				setChannel("installed");
			}
		};
		void load();
	}, []);

	const installUpdate = async (update: Update) => {
		setState({ kind: "installing", applying: false });
		let downloaded = 0;
		let total = 0;

		try {
			await update.downloadAndInstall((event) => {
				switch (event.event) {
					case "Started":
						total = event.data.contentLength ?? 0;
						break;
					case "Progress":
						downloaded += event.data.chunkLength;
						setState({
							kind: "installing",
							applying: false,
							percent: total > 0 ? Math.min(100, Math.floor((downloaded * 100) / total)) : undefined,
						});
						break;
					case "Finished":
						setState({ kind: "installing", applying: false, percent: 100 });
						break;
					default:
						break;
				}
			});
			await relaunch();
		} catch (failure) {
			setState({ kind: "failed", detail: messageOf(failure) });
		}
	};

	// On success the backend restarts the app, so this call does not return; a failure comes back as an error.
	const installPortable = async () => {
		setState({ kind: "installing", applying: false });
		const progress = newProgressChannel<PortableUpdateStep>((step) => {
			if (step.step === "downloading") {
				setState({
					kind: "installing",
					applying: false,
					percent: step.percent ?? undefined,
				});
			} else {
				setState({ kind: "installing", applying: true });
			}
		});

		try {
			await unwrap(commands.portableUpdateInstall(progress));
		} catch (failure) {
			setState({ kind: "failed", detail: messageOf(failure) });
		}
	};

	const checkNow = async () => {
		setState({ kind: "checking" });

		try {
			if (channel === "portable") {
				const found = await unwrap(commands.portableUpdateCheck());
				setState(
					found
						? {
								kind: "available",
								version: found.version,
								install: () => void installPortable(),
							}
						: { kind: "current" },
				);
				return;
			}

			const update = await check();
			setState(
				update
					? {
							kind: "available",
							version: update.version,
							install: () => void installUpdate(update),
						}
					: { kind: "current" },
			);
		} catch (failure) {
			setState({ kind: "failed", detail: messageOf(failure) });
		}
	};

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

import { CircleAlert, CircleCheck, CircleOff, FileUp, PackageSearch } from "lucide-react";
import { useState } from "react";

import { useWorkspace } from "@/app/contexts/workspace-context";
import { type CommunityProfileMod, type CommunityProfilePreview, commands } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { registerTask, removeTask } from "@/lib/tasks-store";
import { pickFiles } from "@/lib/tauri";

const ICON = {
	matched: CircleCheck,
	other_version: CircleOff,
	missing: PackageSearch,
} as const satisfies Record<CommunityProfileMod["state"], typeof CircleCheck>;

const TONE = {
	matched: "text-muted-foreground",
	other_version: "text-foreground",
	missing: "text-destructive",
} as const satisfies Record<CommunityProfileMod["state"], string>;

/// Reads an r2modman or Gale profile before anything changes: the packages it names, the ones the library has, the
/// ones it does not, and the versions that differ. A confirm creates a normal profile with the matched mods.
export function CommunityProfileDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { gameActions } = useWorkspace();
	const [path, setPath] = useState<string | null>(null);
	const [preview, setPreview] = useState<CommunityProfilePreview | null>(null);
	const [name, setName] = useState("");
	const [busy, setBusy] = useState(false);
	const [failure, setFailure] = useState<string | null>(null);

	const pick = async () => {
		setFailure(null);
		const [file] = await pickFiles(t("community.filter"), ["yml", "yaml"], false);
		if (!file) return;
		setBusy(true);
		try {
			const inspected = await unwrap(commands.inspectCommunityProfile(gameId, file));
			setPath(file);
			setPreview(inspected);
			setName(inspected.profile);
		} catch (error) {
			setFailure(messageOf(error));
		}
		// Both the success and the failure path end the busy state here; React Compiler skips a `finally`.
		setBusy(false);
	};

	const importProfile = async () => {
		if (!path) return;
		const id = `community-profile:${gameId}|${path}`;
		registerTask({
			id,
			game: gameId,
			kind: "install",
			title: t("community.importing"),
			detail: name,
			state: "running",
			source: "local",
		});
		setBusy(true);
		setFailure(null);
		let imported = false;
		let problem: string | null = null;
		try {
			imported = await gameActions.importCommunityProfile(path, name);
		} catch (error) {
			problem = messageOf(error);
		}
		removeTask(id);
		setBusy(false);
		if (imported) onClose();
		else setFailure(problem ?? t("community.importFailed"));
	};

	const installable = preview?.mods.filter((item) => item.folder).length ?? 0;

	return (
		<ModalDialog title={t("community.title")} description={t("community.hint")} width="xl" onClose={onClose}>
			<div className="flex flex-wrap items-center gap-2">
				<Button variant="outline" disabled={busy} onClick={() => void pick()}>
					<FileUp aria-hidden />
					{t("community.choose")}
				</Button>
				{path && <span className="text-muted-foreground font-mono text-xs break-all">{path}</span>}
			</div>

			{preview && (
				<>
					<div className="flex flex-wrap items-end gap-2">
						<Field className="min-w-48 flex-1">
							<FieldLabel htmlFor="community-profile-name" className="text-muted-foreground text-xs">
								{t("community.profileName")}
							</FieldLabel>
							<Input id="community-profile-name" value={name} onChange={(event) => setName(event.currentTarget.value)} />
						</Field>
						<span className="text-muted-foreground text-sm tabular-nums">
							{t("community.summary", {
								mods: preview.mods.length,
								installable,
								missing: preview.missing,
								versions: preview.other_versions,
							})}
						</span>
					</div>

					{preview.has_configs && <p className="text-muted-foreground text-xs">{t("community.configs")}</p>}

					<BoxList scroll tall>
						{preview.mods.map((item) => {
							const Icon = ICON[item.state];
							return (
								<BoxListRow key={`${item.name}:${item.version}`}>
									<ItemMedia className="self-start">
										<Icon className={`size-4 ${TONE[item.state]}`} aria-hidden />
									</ItemMedia>
									<ItemContent className="min-w-0 gap-0.5">
										<div className="flex flex-wrap items-center gap-2">
											<ItemTitle className="min-w-0 truncate font-normal">{item.name}</ItemTitle>
											<span className="text-muted-foreground font-mono text-xs">{item.version || "—"}</span>
											<span className="text-muted-foreground text-xs">
												{item.enabled ? t("community.enabled") : t("community.disabled")}
											</span>
										</div>
										{item.state === "other_version" && (
											<ItemDescription>{t("community.otherVersion", { version: item.selected_version ?? "—" })}</ItemDescription>
										)}
										{item.state === "missing" && <ItemDescription className="text-destructive">{t("community.missing")}</ItemDescription>}
									</ItemContent>
								</BoxListRow>
							);
						})}
					</BoxList>
				</>
			)}

			{failure && (
				<FieldError className="flex items-start gap-2">
					<CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
					{failure}
				</FieldError>
			)}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.close")}
				</Button>
				{preview && installable > 0 && (
					<Button disabled={busy || name.trim() === ""} onClick={() => void importProfile()}>
						<FileUp aria-hidden />
						{busy ? t("community.importing") : t("community.import", { count: installable })}
					</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}

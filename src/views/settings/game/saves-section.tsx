import { FilePlus2, FolderOpen, FolderPlus, Trash2, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { Status } from "@/bindings";
import { HelpHint } from "@/components/app/help-hint";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { Item, ItemActions, ItemContent } from "@/components/ui/item";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import { formatSize } from "@/lib/mods";
import { Section, SectionBlock } from "@/views/settings/controls";

/// The actions and the running flag of the game settings page for the user-data list.
export interface UserDataActions {
	onSetUserData: (set: string, isolated: boolean) => void | Promise<void>;
	/// The size of the profile's own layer for one set, read when the section opens.
	onUserDataSize: (set: string) => Promise<number>;
	/// Moves the layer back into the game folder and makes the set shared.
	onMoveUserData: (set: string) => void | Promise<void>;
	/// The game runs, so it still reads the old layer; the move waits.
	gameRunning: boolean;
}

/// What the game keeps outside its folder, and the files that follow each profile. Both act at once: a switch moves the
/// profile's copy of the data, and adding or removing a file changes the profile's list straight away.
export function SavesSection({
	status,
	userData,
	onOpen,
	profileData,
	onAddProfileData,
	onRemoveProfileData,
}: {
	status: Status;
	userData: UserDataActions;
	onOpen: (path: string) => void | Promise<void>;
	profileData: string[];
	onAddProfileData: (directory: boolean) => void | Promise<void>;
	onRemoveProfileData: (path: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const { onSetUserData, onUserDataSize, onMoveUserData, gameRunning } = userData;
	const active = status.profiles.find((entry) => entry.active);
	const profile = active?.name ?? "";
	const profileId = active?.id ?? "";
	const [moving, setMoving] = useState<{ id: string; label: string; upper: string; path: string } | null>(null);

	return (
		<Section
			id="settings-saves"
			title={t("settings.saves.title")}
			hint={t("settings.saves.hint")}
			help={t("settings.userData.hint", { profile })}
		>
			{status.user_data.length > 0 && (
				<SectionBlock>
					<ul className="grid gap-3">
						{status.user_data.map((set) => {
							const switchId = `user-data-${set.id}`;
							const label = t(`settings.userData.kinds.${set.kind}`);
							return (
								<Item asChild key={set.id} variant="outline" className="grid items-stretch gap-2">
									<li>
										<div className="flex flex-wrap items-center gap-2">
											<span className="text-sm font-medium">{label}</span>
											<Badge variant="outline">{t(`settings.userData.mode.${set.mode}`)}</Badge>
											{!set.available && (
												<Badge variant="outline" className="text-muted-foreground">
													{t("settings.userData.missing")}
												</Badge>
											)}
										</div>
										<p className="text-muted-foreground font-mono text-xs break-all">{set.path}</p>
										{set.isolated && set.mode === "redirect" && profileId !== "" && (
											<UserDataSize key={`${profileId}:${set.id}`} setId={set.id} onSize={onUserDataSize} />
										)}
										<div className="flex flex-wrap items-center justify-between gap-3">
											<div className="flex items-center gap-2">
												<Switch
													id={switchId}
													type="button"
													checked={set.isolated}
													onCheckedChange={(checked) => void onSetUserData(set.id, checked)}
												/>
												<Label htmlFor={switchId}>{t("settings.userData.isolated")}</Label>
											</div>
											{set.isolated && set.mode === "redirect" && (
												<div className="flex flex-wrap items-center gap-2">
													{set.upper_exists && (
														<Button type="button" variant="outline" size="sm" onClick={() => void onOpen(set.upper)}>
															<FolderOpen /> {t("settings.userData.open")}
														</Button>
													)}
													<Button
														type="button"
														variant="outline"
														size="sm"
														disabled={gameRunning}
														onClick={() => setMoving({ id: set.id, label, upper: set.upper, path: set.path })}
													>
														<Undo2 /> {t("settings.userData.move")}
													</Button>
												</div>
											)}
										</div>
										{set.isolated && set.inherit && <p className="text-muted-foreground text-xs">{t("settings.userData.inherit")}</p>}
										{set.isolated && !set.inherit && <p className="text-muted-foreground text-xs">{t("settings.userData.emptyBase")}</p>}
										{set.mode === "copy" && <p className="text-muted-foreground text-xs">{t("settings.userData.copy")}</p>}
										{set.isolated && set.mode === "redirect" && gameRunning && (
											<p className="text-muted-foreground text-xs">{t("settings.userData.moveRunning")}</p>
										)}
										{set.isolated && set.mode === "redirect" && status.deployed && (
											<p className="text-muted-foreground text-xs">{t("settings.userData.moveDeployed")}</p>
										)}
									</li>
								</Item>
							);
						})}
					</ul>
				</SectionBlock>
			)}

			<SectionBlock>
				<div className="grid gap-3">
					<div className="grid gap-1">
						<div className="flex items-center gap-1">
							<h3 className="text-sm font-medium">{t("settings.profileData.title")}</h3>
							<HelpHint>{t("settings.profileData.help")}</HelpHint>
						</div>
						<p className="text-muted-foreground text-xs text-pretty">{t("settings.profileData.hint")}</p>
					</div>
					{profileData.length === 0 ? (
						<Empty className="border border-dashed">
							<EmptyHeader>
								<EmptyDescription>{t("settings.profileData.empty")}</EmptyDescription>
							</EmptyHeader>
						</Empty>
					) : (
						<ul className="grid gap-2">
							{profileData.map((path) => (
								<Item asChild key={path} variant="outline" size="sm" className="min-w-0">
									<li>
										<ItemContent className="min-w-0">
											<span className="min-w-0 truncate font-mono text-xs" title={path}>
												{path}
											</span>
										</ItemContent>
										<ItemActions>
											<Button
												type="button"
												variant="ghost"
												size="icon-sm"
												aria-label={t("settings.profileData.remove", { path })}
												onClick={() => void onRemoveProfileData(path)}
											>
												<Trash2 />
											</Button>
										</ItemActions>
									</li>
								</Item>
							))}
						</ul>
					)}
					{status.game_profile_data.length > 0 && (
						<div className="grid gap-1.5">
							<p className="text-muted-foreground text-xs">{t("settings.profileData.fromGame")}</p>
							<ul className="grid gap-1">
								{status.game_profile_data.map((path) => (
									<Item asChild key={path} size="xs" variant="muted" className="py-1">
										<li>
											<ItemContent>
												<span className="truncate font-mono text-xs" title={path}>
													{path}
												</span>
											</ItemContent>
										</li>
									</Item>
								))}
							</ul>
						</div>
					)}
					<div className="flex flex-wrap gap-2">
						<Button type="button" variant="outline" size="sm" onClick={() => void onAddProfileData(false)}>
							<FilePlus2 /> {t("settings.profileData.addFile")}
						</Button>
						<Button type="button" variant="outline" size="sm" onClick={() => void onAddProfileData(true)}>
							<FolderPlus /> {t("settings.profileData.addFolder")}
						</Button>
					</div>
				</div>
			</SectionBlock>

			<ConfirmDialog
				open={moving !== null}
				onOpenChange={(open) => {
					if (!open) setMoving(null);
				}}
				title={t("settings.userData.moveTitle", { set: moving?.label ?? "" })}
				description={t("settings.userData.moveBody", {
					upper: moving?.upper ?? "",
					path: moving?.path ?? "",
				})}
				confirmLabel={t("settings.userData.moveConfirm")}
				destructive={false}
				onConfirm={async () => {
					const id = moving?.id;
					setMoving(null);

					if (id) await onMoveUserData(id);
				}}
			/>
		</Section>
	);
}

/// The profile's own size for one set. It is read on demand, so opening the settings page never walks the layer; the
/// line stays out until the answer arrives, and a failed read leaves it out. The caller keys it by profile and set, so a
/// switch starts from an empty line instead of the previous profile's number.
function UserDataSize({ setId, onSize }: { setId: string; onSize: (set: string) => Promise<number> }) {
	const { t, i18n } = useTranslation();
	const [size, setSize] = useState<number | null>(null);

	useEffect(() => {
		let cancelled = false;
		const load = async () => {
			try {
				const kb = await onSize(setId);

				if (!cancelled) setSize(kb);
			} catch {
				// A failed read keeps the line out; the set itself is still usable.
			}
		};
		void load();
		return () => {
			cancelled = true;
		};
	}, [onSize, setId]);

	if (size === null) return null;
	return (
		<p className="text-muted-foreground font-mono text-xs tabular-nums">
			{t("settings.userData.size", { size: formatSize(size, i18n.language) })}
		</p>
	);
}

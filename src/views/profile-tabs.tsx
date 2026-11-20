import { ArrowLeftRight, ChevronDown, Copy, EllipsisVertical, Link, Package, PackageOpen, Pencil, Plus, Trash2 } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import type { ProfileView } from "@/bindings";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import { freeName } from "@/lib/profile-names";

const ProfileCompareDialog = lazy(async () => {
	const { ProfileCompareDialog: component } = await import("@/views/profile-compare");
	return { default: component };
});

const ProfileCopyDialog = lazy(async () => {
	const { ProfileCopyDialog: component } = await import("@/views/profile-copy-dialog");
	return { default: component };
});

/// Compact profile switcher next to the game name: shows the active profile and lets the user switch.
export function ProfileSwitcher({
	profiles,
	onSwitch,
	onCreate,
	busy,
	gameRunning,
}: {
	profiles: ProfileView[];
	onSwitch: (id: string) => void | Promise<void>;
	onCreate: (name: string, copyFrom: string | null, copyData?: boolean) => void | Promise<void>;
	/// A long action is running; a switch waits until it ends.
	busy: boolean;
	/// The game runs with the profile it started with; switching only changes the list shown and edited.
	gameRunning: boolean;
}) {
	const { t } = useTranslation();
	const names = profiles.map((profile) => profile.name);
	const active = profiles.find((profile) => profile.active) ?? profiles[0] ?? null;

	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="outline" aria-label={t("profiles.navLabel")} className="max-w-56 justify-start" disabled={busy}>
					{active?.deployed && <span aria-hidden className="bg-applied size-2 shrink-0 rounded-full" />}
					<span className="min-w-0 truncate">{active?.name ?? t("profiles.defaultName", { number: 1 })}</span>
					<ChevronDown className="shrink-0" />
					{active && (
						<span className="text-subtle-foreground shrink-0 text-xs tabular-nums">
							{t("profiles.enabledCount", {
								enabled: active.enabled_count,
								total: active.mod_count,
							})}
						</span>
					)}
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-64">
				{gameRunning && (
					<>
						<DropdownMenuLabel className="text-muted-foreground text-xs font-normal">{t("profiles.switchWhileRunning")}</DropdownMenuLabel>
						<DropdownMenuSeparator />
					</>
				)}
				{profiles.map((profile) => (
					<DropdownMenuItem
						key={profile.id}
						disabled={busy}
						onSelect={() => void onSwitch(profile.id)}
						className={profile.active ? "font-semibold" : undefined}
					>
						{profile.deployed && <span aria-hidden className="bg-applied size-2 shrink-0 rounded-full" />}
						{profile.name}
						<span className="text-subtle-foreground ml-auto text-xs tabular-nums">
							{t("profiles.enabledCount", {
								enabled: profile.enabled_count,
								total: profile.mod_count,
							})}
						</span>
					</DropdownMenuItem>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuItem
					disabled={busy}
					onSelect={() =>
						void onCreate(
							freeName((number) => t("profiles.defaultName", { number }), names),
							null,
							false,
						)
					}
				>
					<Plus /> {t("profiles.add")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

/// Profile management for the active profile: a compact menu next to the profile switcher, not a block of buttons in
/// the inspector. Rename, duplicate, compare, packs and delete all act on the profile the header already shows.
export function ProfileActions({
	gameId,
	profiles,
	onCreate,
	onCopyData,
	onRename,
	onDelete,
	onExportPack,
	onImportPack,
	onImportPackLink,
}: {
	gameId: string;
	profiles: ProfileView[];
	onCreate: (name: string, copyFrom: string | null, copyData?: boolean) => void | Promise<void>;
	onCopyData: (source: string, target: string) => void | Promise<void>;
	onRename: (id: string, name: string) => void | Promise<void>;
	onDelete: (id: string) => void | Promise<void>;
	onExportPack: () => void | Promise<void>;
	onImportPack: () => void | Promise<void>;
	onImportPackLink: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [renaming, setRenaming] = useState<ProfileView | null>(null);
	const [comparing, setComparing] = useState(false);
	const [copying, setCopying] = useState(false);
	const [deleting, setDeleting] = useState<ProfileView | null>(null);
	const names = profiles.map((profile) => profile.name);
	const active = profiles.find((profile) => profile.active) ?? profiles[0] ?? null;

	const deleteHint = (profile: ProfileView) => {
		if (profiles.length <= 1) return t("profiles.delete.hintLast");

		if (profile.deployed) return t("profiles.delete.hintDeployed");

		if (profile.has_user_data) return t("profiles.delete.hintUserData");
		return t("profiles.delete.hintListOnly");
	};

	if (!active) return null;

	return (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button variant="ghost" size="icon" aria-label={t("profiles.actions")} title={t("profiles.actions")}>
						<EllipsisVertical />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" className="w-56">
					<DropdownMenuLabel>{t("profiles.menu.profile")}</DropdownMenuLabel>
					<DropdownMenuItem onSelect={() => setRenaming(active)}>
						<Pencil /> {t("profiles.rename.action")}
					</DropdownMenuItem>
					<DropdownMenuItem
						onSelect={() =>
							void onCreate(
								freeName((number) => t("profiles.copyName", { name: active.name, number }), names),
								active.id,
								false,
							)
						}
					>
						<Copy /> {t("profiles.duplicate")}
					</DropdownMenuItem>
					<DropdownMenuItem
						onSelect={() =>
							void onCreate(
								freeName((number) => t("profiles.copyName", { name: active.name, number }), names),
								active.id,
								true,
							)
						}
					>
						<Copy /> {t("profiles.duplicateWithData")}
					</DropdownMenuItem>
					{profiles.length > 1 && (
						<DropdownMenuItem onSelect={() => setComparing(true)}>
							<ArrowLeftRight /> {t("profiles.compare.button")}
						</DropdownMenuItem>
					)}
					{profiles.length > 1 && (
						<DropdownMenuItem onSelect={() => setCopying(true)}>
							<Copy /> {t("profiles.copy.menu")}
						</DropdownMenuItem>
					)}
					<DropdownMenuSeparator />
					<DropdownMenuLabel>{t("profiles.menu.pack")}</DropdownMenuLabel>
					<DropdownMenuItem onSelect={() => void onExportPack()}>
						<Package /> {t("pack.export")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={() => void onImportPack()}>
						<PackageOpen /> {t("pack.import")}
					</DropdownMenuItem>
					<DropdownMenuItem onSelect={() => void onImportPackLink()}>
						<Link /> {t("pack.link.menu")}
					</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem variant="destructive" disabled={profiles.length <= 1} onSelect={() => setDeleting(active)}>
						<Trash2 /> {t("profiles.delete.action")}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
			<Suspense fallback={null}>
				<ProfileCompareDialog gameId={gameId} profiles={profiles} active={active} open={comparing} onClose={() => setComparing(false)} />
				<ProfileCopyDialog
					profiles={profiles}
					target={active}
					open={copying}
					onClose={() => setCopying(false)}
					onCopy={(source) => onCopyData(source, active.id)}
				/>
			</Suspense>
			<RenameDialog key={renaming?.id ?? ""} profile={renaming} taken={names} onClose={() => setRenaming(null)} onRename={onRename} />
			<ConfirmDialog
				open={deleting !== null}
				onOpenChange={(open) => !open && setDeleting(null)}
				title={t("profiles.delete.title", { name: deleting?.name ?? "" })}
				description={deleting ? deleteHint(deleting) : ""}
				confirmLabel={t("profiles.delete.action")}
				confirmDisabled={!deleting || profiles.length <= 1}
				onConfirm={() => {
					if (deleting) void onDelete(deleting.id);
				}}
			/>
		</>
	);
}

function RenameDialog({
	profile,
	taken,
	onClose,
	onRename,
}: {
	profile: ProfileView | null;
	taken: string[];
	onClose: () => void;
	onRename: (id: string, name: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [name, setName] = useState(profile?.name ?? "");
	const trimmed = name.trim();
	const clashes = taken.some(
		(existing) => existing !== profile?.name && existing.trim().toLocaleLowerCase() === trimmed.toLocaleLowerCase(),
	);
	const unchanged = trimmed === profile?.name;

	// The footer sits outside the form, so the submit button names the form by id; Enter in the field still submits it.
	return (
		<ModalDialog title={t("profiles.rename.title")} width="sm" open={profile !== null} onClose={onClose}>
			<form
				id="rename-profile-form"
				className="grid gap-3"
				onSubmit={(event) => {
					event.preventDefault();

					if (!(profile && trimmed) || unchanged || clashes) return;
					void onRename(profile.id, trimmed);
					onClose();
				}}
			>
				<Field data-invalid={clashes}>
					<FieldLabel htmlFor="rename-profile">{t("profiles.rename.label")}</FieldLabel>
					<Input id="rename-profile" value={name} aria-invalid={clashes} onChange={(event) => setName(event.currentTarget.value)} />
					{clashes && <FieldError>{t("profiles.rename.taken")}</FieldError>}
				</Field>
			</form>
			<DialogFooter>
				<Button type="button" variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button type="submit" form="rename-profile-form" disabled={!trimmed || unchanged || clashes}>
					{t("profiles.rename.action")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

import { useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { ModGroup, ModGroupKind } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";

const KINDS = ["bundle", "translation"] as const satisfies readonly ModGroupKind[];

/// A fresh id that cannot collide with a group already saved on the profile.
export function nextGroupId(groups: readonly ModGroup[]): string {
	const used = new Set(groups.map((group) => group.id));
	let index = 1;

	while (used.has(`grp${index}`)) index += 1;
	return `grp${index}`;
}

/// Creates a group from the selected families, or renames one that is already saved. The full array is sent so the save
/// joins the profile queue like any other list edit.
export function GroupEditDialog({ id, members, onClose }: { id?: string; members?: string[]; onClose: () => void }) {
	const { t } = useTranslation();
	const { status, gameActions } = useWorkspace();
	const existing = id === undefined ? undefined : status?.groups.find((group) => group.id === id);
	const [name, setName] = useState(existing?.name ?? "");
	const [kind, setKind] = useState<ModGroupKind>(existing?.kind ?? "bundle");
	const [language, setLanguage] = useState(existing?.language ?? "");
	// A saved group keeps its own members; a new one takes the ids the selection bar passed.
	const chosen = existing?.members ?? members ?? [];
	const canSave = name.trim() !== "" && (existing !== undefined || chosen.length >= 2);

	const save = () => {
		const groups = status?.groups ?? [];
		const group: ModGroup = {
			id: existing?.id ?? nextGroupId(groups),
			name: name.trim(),
			members: chosen,
			kind,
			language: kind === "translation" ? language.trim() || null : null,
		};
		const next = existing ? groups.map((entry) => (entry.id === existing.id ? group : entry)) : [...groups, group];
		onClose();
		void gameActions.saveModGroups(next);
	};

	return (
		<ModalDialog
			title={t(existing ? "mods.group.editTitle" : "mods.group.createTitle")}
			description={t("mods.group.hint")}
			width="sm"
			onClose={onClose}
		>
			<FieldGroup className="gap-4">
				<Field>
					<FieldLabel htmlFor="group-name">{t("mods.group.name")}</FieldLabel>
					<Input
						id="group-name"
						autoFocus
						value={name}
						placeholder={t("mods.group.namePlaceholder")}
						onChange={(event) => setName(event.currentTarget.value)}
					/>
				</Field>
				<Field>
					<FieldLabel htmlFor="group-kind">{t("mods.group.kind")}</FieldLabel>
					<Select value={kind} onValueChange={(value) => setKind(KINDS.find((choice) => choice === value) ?? "bundle")}>
						<SelectTrigger id="group-kind">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{KINDS.map((choice) => (
								<SelectItem key={choice} value={choice}>
									{t(`mods.group.kindChoice.${choice}`)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</Field>
				{kind === "translation" && (
					<Field>
						<FieldLabel htmlFor="group-language">{t("mods.group.language")}</FieldLabel>
						<Input
							id="group-language"
							value={language}
							maxLength={16}
							placeholder={t("mods.group.languagePlaceholder")}
							onChange={(event) => setLanguage(event.currentTarget.value)}
						/>
					</Field>
				)}
				{existing === undefined && chosen.length < 2 && <p className="text-muted-foreground text-xs">{t("mods.group.needMembers")}</p>}
			</FieldGroup>
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("mods.group.cancel")}
				</Button>
				<Button disabled={!canSave} onClick={save}>
					{t("mods.group.save")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

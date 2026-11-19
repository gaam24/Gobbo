import { useState } from "react";
import type { ModInfoEdit } from "@/bindings";
import { Disclosure } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// The name, version and author of the selected version, typed by hand when `info.json` is missing or wrong. It sits in a
/// disclosure: the Overview already shows these values, so the form is only opened to change them.
/// The form is keyed by the saved values, so after a save or a reset it shows what the mod now has.
export function InfoSection({
	mod,
	onSetInfo,
}: {
	mod: Mod;
	onSetInfo: (modId: string, folder: string, info: ModInfoEdit) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [name, setName] = useState(mod.info.name);
	const [version, setVersion] = useState(mod.info.version ?? "");
	const [author, setAuthor] = useState(mod.info.author ?? "");
	const changed = name.trim() !== mod.info.name || version.trim() !== (mod.info.version ?? "") || author.trim() !== (mod.info.author ?? "");

	/// An empty field sends `null`, which goes back to the value from the file.
	const save = (info: ModInfoEdit) => void onSetInfo(mod.id, mod.folder, info);

	return (
		<Disclosure title={t("mods.edit.title")}>
			<section className="grid gap-3">
				<p className="text-muted-foreground text-xs text-pretty">{t("mods.edit.hint")}</p>
				<FieldGroup className="gap-3">
					<Field>
						<FieldLabel htmlFor="mod-info-name">{t("mods.edit.name")}</FieldLabel>
						<Input id="mod-info-name" value={name} onChange={(event) => setName(event.currentTarget.value)} />
					</Field>
					<Field>
						<FieldLabel htmlFor="mod-info-version">{t("mods.edit.version")}</FieldLabel>
						<Input
							id="mod-info-version"
							className="font-mono"
							value={version}
							onChange={(event) => setVersion(event.currentTarget.value)}
						/>
					</Field>
					<Field>
						<FieldLabel htmlFor="mod-info-author">{t("mods.edit.author")}</FieldLabel>
						<Input id="mod-info-author" value={author} onChange={(event) => setAuthor(event.currentTarget.value)} />
					</Field>
				</FieldGroup>
				<div className="flex flex-wrap gap-2">
					<Button
						variant="outline"
						size="sm"
						disabled={!changed}
						onClick={() => save({ name: name.trim() || null, version: version.trim() || null, author: author.trim() || null })}
					>
						{t("mods.edit.save")}
					</Button>
					<Button variant="ghost" size="sm" onClick={() => save({ name: null, version: null, author: null })}>
						{t("mods.edit.reset")}
					</Button>
				</div>
			</section>
		</Disclosure>
	);
}

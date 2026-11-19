import { useState } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import type { FullMod } from "@/lib/mods";

/// The user's note about a mod. It is saved with the button once the text has changed.
export function NoteSection({ mod, onSetNote }: { mod: FullMod; onSetNote: (modId: string, note: string) => void | Promise<void> }) {
	const { t } = useTranslation();
	const [value, setValue] = useState(mod.notes);
	const changed = value !== mod.notes;

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.notes.title")} />
			<Field>
				<FieldLabel htmlFor="mod-note" className="sr-only">
					{t("mods.notes.title")}
				</FieldLabel>
				<Textarea
					id="mod-note"
					value={value}
					rows={3}
					placeholder={t("mods.notes.placeholder")}
					onChange={(event) => setValue(event.currentTarget.value)}
				/>
			</Field>
			<Button variant="outline" size="sm" className="justify-self-start" disabled={!changed} onClick={() => void onSetNote(mod.id, value)}>
				{t("mods.notes.save")}
			</Button>
		</section>
	);
}

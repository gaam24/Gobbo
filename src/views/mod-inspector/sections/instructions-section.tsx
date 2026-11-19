import { useState } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// What players do to install or set up the mod. It goes into the exported mod list and the copied list.
export function InstructionsSection({
	mod,
	onSetInstructions,
}: {
	mod: Mod;
	onSetInstructions: (modId: string, text: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [value, setValue] = useState(mod.instructions);
	const changed = value.trim() !== mod.instructions;

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.instructions.title")} hint={t("mods.instructions.hint")} />
			<Field>
				<FieldLabel htmlFor="mod-instructions" className="sr-only">
					{t("mods.instructions.title")}
				</FieldLabel>
				<Textarea
					id="mod-instructions"
					value={value}
					rows={4}
					placeholder={t("mods.instructions.placeholder")}
					onChange={(event) => setValue(event.currentTarget.value)}
				/>
			</Field>
			<Button
				variant="outline"
				size="sm"
				className="justify-self-start"
				disabled={!changed}
				onClick={() => void onSetInstructions(mod.id, value)}
			>
				{t("mods.instructions.save")}
			</Button>
		</section>
	);
}

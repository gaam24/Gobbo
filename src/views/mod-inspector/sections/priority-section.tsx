import { useState } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// The load priority the user sets for a mod. It goes into the game's settings file; an empty field leaves the order to the game.
export function PrioritySection({
	mod,
	onSetPriority,
}: {
	mod: Mod;
	onSetPriority: (modId: string, priority: number | null) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const saved = mod.load_priority ?? null;
	const [value, setValue] = useState(saved === null ? "" : String(saved));
	const trimmed = value.trim();
	const parsed = trimmed === "" ? null : Number(trimmed);
	const valid = parsed === null || Number.isInteger(parsed);
	const canSave = valid && parsed !== saved;

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.priority.title")} hint={t("mods.priority.hint")} />
			<Field>
				<FieldLabel htmlFor="mod-priority" className="sr-only">
					{t("mods.priority.title")}
				</FieldLabel>
				<div className="flex flex-wrap items-center gap-2">
					<Input
						id="mod-priority"
						type="number"
						inputMode="numeric"
						step={1}
						placeholder={t("mods.priority.auto")}
						className="h-9 w-32 font-mono tabular-nums"
						value={value}
						onChange={(event) => setValue(event.currentTarget.value)}
					/>
					<Button variant="outline" size="sm" disabled={!canSave} onClick={() => void onSetPriority(mod.id, parsed)}>
						{t("mods.priority.save")}
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={saved === null && trimmed === ""}
						onClick={() => {
							setValue("");
							void onSetPriority(mod.id, null);
						}}
					>
						{t("mods.priority.clear")}
					</Button>
				</div>
			</Field>
		</section>
	);
}

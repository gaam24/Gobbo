import { useState } from "react";
import type { StepChoices } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { useTranslation } from "@/i18n";

/// The game's steps this mod asks for. They run when the game starts from Gobbo, and only for enabled mods.
export function StepsSection({
	steps,
	onSave,
}: {
	steps: StepChoices;
	onSave: (beforeStart: string[], afterEnd: string[]) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const saved = [...steps.before_start, ...steps.after_end];
	const [picked, setPicked] = useState<string[]>(saved);
	const changed = picked.length !== saved.length || picked.some((id) => !saved.includes(id));

	/// The picked steps, in the order the game defines them, split by the phase they run in.
	const save = () => {
		const chosen = steps.defined.filter((step) => picked.includes(step.id));
		void onSave(
			chosen.filter((step) => step.phase === "before_start").map((step) => step.id),
			chosen.filter((step) => step.phase === "after_end").map((step) => step.id),
		);
	};

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.steps.title")} hint={t("mods.steps.hint")} />
			<BoxList>
				{steps.defined.map((step) => (
					<BoxListRow key={step.id}>
						<Checkbox
							id={`mod-step-${step.id}`}
							checked={picked.includes(step.id)}
							onCheckedChange={(checked) => {
								const on = checked === true;
								setPicked((current) => (on ? [...current, step.id] : current.filter((id) => id !== step.id)));
							}}
						/>
						<Label htmlFor={`mod-step-${step.id}`} className="grid min-w-0 flex-1 gap-0.5 font-normal">
							<span className="text-sm font-medium">{step.id}</span>
							<span className="text-muted-foreground truncate font-mono text-xs">{step.program}</span>
						</Label>
						<span className="text-subtle-foreground shrink-0 text-xs">{t(`runs.phase.${step.phase}`)}</span>
					</BoxListRow>
				))}
			</BoxList>
			<Button variant="outline" size="sm" className="justify-self-start" disabled={!changed} onClick={save}>
				{t("mods.steps.save")}
			</Button>
		</section>
	);
}

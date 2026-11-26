import { cn } from "cn";
import { Check } from "lucide-react";
import type { ReactNode } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { useTranslation } from "@/i18n";

/// The steps this card can highlight. Only finishing setup has no id here; the header owns that action. The fix step is
/// informational: it is shown as current when the backend says fix-problems and has no button, so no tick claims the
/// header's action.
export type StepId = "add" | "enable" | "loader" | "fix" | "deploy" | null;

/// The way to a first deployment, as steps with a tick. The highlighted step is the backend's next action; the ticks
/// only draw progress and stay decorative.
export function GettingStarted({
	steps,
	currentId,
	actions,
}: {
	steps: Array<{ id: StepId; done: boolean; title: string; hint: string }>;
	currentId: StepId;
	/// The buttons of the step to do now.
	actions: ReactNode;
}) {
	const { t } = useTranslation();
	const current = steps.findIndex((step) => step.id === currentId);
	return (
		<section aria-label={t("start.title")} className="bg-muted grid gap-2 rounded-lg p-4">
			<SectionHeading title={t("start.title")} />
			<ol className="grid gap-3">
				{steps.map((step, index) => (
					<li key={step.title} className="flex gap-3" aria-current={index === current ? "step" : undefined}>
						<span
							aria-hidden
							className={cn(
								"grid size-6 shrink-0 place-items-center rounded-full border text-xs font-semibold tabular-nums",
								step.done && "bg-applied text-background border-transparent",
								index === current && "border-foreground text-foreground",
								!step.done && index !== current && "text-subtle-foreground",
							)}
						>
							{step.done ? <Check className="size-3.5" /> : index + 1}
						</span>
						<div className="grid min-w-0 gap-0.5">
							<p
								className={cn(
									"text-sm font-medium",
									step.done && "text-muted-foreground line-through",
									index > current && current >= 0 && "text-muted-foreground",
								)}
							>
								{step.title}
							</p>
							{index === current && <p className="text-muted-foreground text-xs">{step.hint}</p>}
							{index === current && <div className="mt-2 flex flex-wrap gap-2 empty:hidden">{actions}</div>}
						</div>
					</li>
				))}
			</ol>
		</section>
	);
}

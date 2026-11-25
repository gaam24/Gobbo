import { Check } from "lucide-react";
import { type TranslationKey, useTranslation } from "@/i18n";
import { QuickSection } from "@/views/games/editor/quick-section";
import type { QuickProps } from "@/views/games/editor/quick-types";

/// The section a backend problem names, as a translation key. The message itself is the stable English the validator
/// writes; the label tells the user where to look.
const SECTION_LABELS: Partial<Record<string, TranslationKey>> = {
	general: "editor.sections.general",
	loader: "editor.sections.loader",
	detection: "editor.sections.detection",
	deploy: "editor.sections.deploy",
	profiles: "editor.sections.profiles",
	load_order: "editor.sections.loadOrder",
	tools: "editor.sections.tools",
	steps: "editor.sections.steps",
	extras: "editor.sections.extras",
	source: "editor.sections.source",
};

/// The check before saving. The local issues are translated and immediate; the backend problems group by the section
/// that owns them. While a local issue is enough, the long validator messages stay out of the way.
export function QuickCheck({ check, issues }: QuickProps) {
	const { t } = useTranslation();
	return (
		<QuickSection number={4} title={t("editor.quick.checkTitle")} hint={t("editor.quick.checkHint")}>
			{issues.length > 0 && (
				<ul className="grid gap-1 text-sm">
					{issues.map((issue) => (
						<li key={issue} className="text-destructive">
							{t(`editor.quick.issue.${issue}`)}
						</li>
					))}
				</ul>
			)}
			{issues.length === 0 && check !== null && check.ok && (
				<p className="text-primary flex items-center gap-2 text-sm font-medium">
					<Check aria-hidden className="size-4" /> {t("editor.quick.checkOk")}
				</p>
			)}
			{issues.length === 0 && check !== null && !check.ok && (
				<ul className="grid gap-1 text-sm">
					{check.problems.map((problem) => (
						<li key={`${problem.section}-${problem.message}`} className="text-destructive">
							<span className="text-muted-foreground text-xs">{t(SECTION_LABELS[problem.section] ?? "editor.sections.source")}</span>{" "}
							{problem.message}
						</li>
					))}
				</ul>
			)}
		</QuickSection>
	);
}

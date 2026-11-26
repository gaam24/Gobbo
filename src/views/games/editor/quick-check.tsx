import { Check, CircleDashed, CircleX } from "lucide-react";
import { BoxList, OutcomeRow } from "@/components/app/content-blocks";
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
			{/* A field not filled in yet is a step to do, not an error: it is drawn as a to-do, not in red. */}
			{issues.length > 0 && (
				<BoxList>
					{issues.map((issue) => (
						<OutcomeRow key={issue} icon={CircleDashed} title={t(`editor.quick.issue.${issue}`)} />
					))}
				</BoxList>
			)}
			{issues.length === 0 && check !== null && check.ok && (
				<BoxList>
					<OutcomeRow icon={Check} tone="primary" title={t("editor.quick.checkOk")} />
				</BoxList>
			)}
			{issues.length === 0 && check !== null && !check.ok && (
				<BoxList>
					{check.problems.map((problem) => (
						<OutcomeRow
							key={`${problem.section}-${problem.message}`}
							icon={CircleX}
							tone="destructive"
							title={problem.message}
							detail={t(SECTION_LABELS[problem.section] ?? "editor.sections.source")}
						/>
					))}
				</BoxList>
			)}
		</QuickSection>
	);
}

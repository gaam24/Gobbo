import { Disclosure, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import { AdvancedDeploy } from "@/views/games/editor/advanced-deploy";
import { AdvancedDetection } from "@/views/games/editor/advanced-detection";
import { AdvancedExtras } from "@/views/games/editor/advanced-extras";
import { AdvancedProfiles } from "@/views/games/editor/advanced-profiles";
import { AdvancedTools } from "@/views/games/editor/advanced-tools";
import type { GameDraft } from "@/views/games/editor/draft";

/// Every field the quick part does not show, and the TOML source. The form and the source edit the same draft: a form
/// change replaces the source text, and applying the source replaces the form.
export function AdvancedForm({
	draft,
	onChange,
	folder,
	programs,
	directories,
	source,
	sourceDirty,
	formText,
	sourceError,
	onSourceChange,
	onApplySource,
}: {
	draft: GameDraft;
	onChange: (draft: GameDraft) => void;
	/** The game folder as the user gave it; a browsed program is made relative to it. */
	folder: string;
	/** The programs Gobbo found in the game folder, offered by the program fields. */
	programs: string[];
	directories: string[];
	source: string;
	sourceDirty: boolean;
	formText: string;
	sourceError: string | null;
	onSourceChange: (source: string) => void;
	onApplySource: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<Disclosure className="scroll-mt-6 border-t pt-6" title={<span className="text-base font-semibold">{t("editor.advanced.title")}</span>}>
			<p className="text-muted-foreground text-sm">{t("editor.advanced.hint")}</p>
			<div className="grid gap-10">
				<section id="editor-detection" className="grid scroll-mt-6 gap-4">
					<SectionHeading title={t("editor.advanced.detection")} />
					<AdvancedDetection draft={draft} onChange={onChange} programs={programs} folder={folder} />
				</section>
				<section id="editor-deploy" className="grid scroll-mt-6 gap-4 border-t pt-8">
					<SectionHeading title={t("editor.advanced.deploy")} />
					<AdvancedDeploy draft={draft} onChange={onChange} directories={directories} />
				</section>
				<section id="editor-profiles" className="grid scroll-mt-6 gap-4 border-t pt-8">
					<SectionHeading title={t("editor.advanced.profiles")} />
					<AdvancedProfiles draft={draft} onChange={onChange} />
				</section>
				<section id="editor-extras" className="grid scroll-mt-6 gap-4 border-t pt-8">
					<SectionHeading title={t("editor.advanced.extras")} />
					<AdvancedExtras draft={draft} onChange={onChange} />
				</section>
				<section id="editor-tools" className="grid scroll-mt-6 gap-4 border-t pt-8">
					<SectionHeading title={t("editor.advanced.tools")} />
					<AdvancedTools draft={draft} onChange={onChange} />
				</section>
				<section id="editor-source" className="grid scroll-mt-6 gap-3 border-t pt-8">
					<SectionHeading
						title={t("editor.source.title")}
						actions={
							<Button type="button" variant="outline" size="sm" disabled={!sourceDirty} onClick={() => void onApplySource()}>
								{t("editor.source.apply")}
							</Button>
						}
					/>
					<p className="text-muted-foreground text-xs text-pretty">{t("editor.source.hint")}</p>
					{sourceDirty && <p className="text-primary text-xs">{t("editor.source.pending")}</p>}
					<Textarea
						className="min-h-80 font-mono text-xs"
						aria-label={t("editor.source.title")}
						value={sourceDirty ? source : formText}
						spellCheck={false}
						onChange={(event) => onSourceChange(event.currentTarget.value)}
					/>
					{sourceError && <p className="text-destructive text-xs whitespace-pre-wrap">{sourceError}</p>}
				</section>
			</div>
		</Disclosure>
	);
}

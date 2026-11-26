import { Disclosure } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import { AdvancedDeploy } from "@/views/games/editor/advanced-deploy";
import { AdvancedDetection } from "@/views/games/editor/advanced-detection";
import { AdvancedExtras } from "@/views/games/editor/advanced-extras";
import { AdvancedProfiles } from "@/views/games/editor/advanced-profiles";
import { AdvancedTools } from "@/views/games/editor/advanced-tools";
import type { GameDraft } from "@/views/games/editor/draft";

/// The fields most games never need, each group folded under one sentence that says when to open it. A user reads five
/// lines instead of forty fields and opens only the group their game needs. The Linux-only values are in `AdvancedLinux`.
export function MoreOptions({
	draft,
	onChange,
	folder,
	programs,
	directories,
}: {
	draft: GameDraft;
	onChange: (draft: GameDraft) => void;
	/** The game folder as the user gave it; a browsed program is made relative to it. */
	folder: string;
	/** The programs Gobbo found in the game folder, offered by the program fields. */
	programs: string[];
	directories: string[];
}) {
	const { t } = useTranslation();
	const groups = [
		{
			id: "detection",
			title: t("editor.advanced.detection"),
			hint: t("editor.more.detectionHint"),
			content: <AdvancedDetection draft={draft} onChange={onChange} programs={programs} folder={folder} />,
		},
		{
			id: "deploy",
			title: t("editor.advanced.deploy"),
			hint: t("editor.more.deployHint"),
			content: <AdvancedDeploy draft={draft} onChange={onChange} directories={directories} />,
		},
		{
			id: "profiles",
			title: t("editor.advanced.profiles"),
			hint: t("editor.more.profilesHint"),
			content: <AdvancedProfiles draft={draft} onChange={onChange} />,
		},
		{
			id: "extras",
			title: t("editor.advanced.extras"),
			hint: t("editor.more.extrasHint"),
			content: <AdvancedExtras draft={draft} onChange={onChange} />,
		},
		{
			id: "tools",
			title: t("editor.advanced.tools"),
			hint: t("editor.more.toolsHint"),
			content: <AdvancedTools draft={draft} onChange={onChange} />,
		},
	];

	return (
		<div className="grid gap-2">
			<p className="text-muted-foreground mt-1 mb-4 text-sm text-pretty">{t("editor.more.hint")}</p>
			{groups.map((group) => (
				<section key={group.id} id={`editor-${group.id}`} className="grid scroll-mt-6 gap-1 border-t py-4">
					<Disclosure title={<span className="text-base font-semibold">{group.title}</span>}>
						<div className="pt-2">{group.content}</div>
					</Disclosure>
					<p className="text-muted-foreground pl-6 text-sm text-pretty">{group.hint}</p>
				</section>
			))}
		</div>
	);
}

/// The definition as TOML. The form and the source edit the same draft: a form change replaces the source text, and
/// applying the source replaces the form.
export function DefinitionSource({
	source,
	sourceDirty,
	formText,
	sourceError,
	onSourceChange,
	onApplySource,
}: {
	source: string;
	sourceDirty: boolean;
	formText: string;
	sourceError: string | null;
	onSourceChange: (source: string) => void;
	onApplySource: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<section id="editor-source" className="mt-1 grid gap-3">
			<div className="flex items-start justify-between gap-3">
				<p className="text-muted-foreground text-sm text-pretty">{t("editor.source.hint")}</p>
				<Button type="button" variant="outline" size="sm" disabled={!sourceDirty} onClick={() => void onApplySource()}>
					{t("editor.source.apply")}
				</Button>
			</div>
			{sourceDirty && <p className="text-primary text-xs">{t("editor.source.pending")}</p>}
			<Textarea
				className="min-h-[60vh] font-mono text-xs"
				aria-label={t("editor.source.title")}
				value={sourceDirty ? source : formText}
				spellCheck={false}
				onChange={(event) => onSourceChange(event.currentTarget.value)}
			/>
			{sourceError && <p className="text-destructive text-xs whitespace-pre-wrap">{sourceError}</p>}
		</section>
	);
}

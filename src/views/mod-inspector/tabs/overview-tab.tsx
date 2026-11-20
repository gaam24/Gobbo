import { Trash2 } from "lucide-react";
import type { StepChoices, UpdateReport, VersionView } from "@/bindings";
import { Disclosure } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { authorOf, choicesText, type FullMod, formatCount, formatDate, formatSize, type Mod } from "@/lib/mods";
import { usePageStatus } from "@/sources/status";
import { FactList, fact } from "@/views/mod-inspector/fact-list";
import type { ModActions } from "@/views/mod-inspector/mod-details";
import { InfoSection } from "@/views/mod-inspector/sections/info-section";
import { InstructionsSection } from "@/views/mod-inspector/sections/instructions-section";
import { NoteSection } from "@/views/mod-inspector/sections/note-section";
import { PrioritySection } from "@/views/mod-inspector/sections/priority-section";
import { RelationsSection } from "@/views/mod-inspector/sections/relations-section";
import { StepsSection } from "@/views/mod-inspector/sections/steps-section";
import { TagsSection } from "@/views/mod-inspector/sections/tags-section";
import { VersionSection } from "@/views/mod-inspector/sections/version-section";

interface OverviewTabProps
	extends Pick<
		ModActions,
		| "onSelectVersion"
		| "onSetTags"
		| "onSetNote"
		| "onSetPriority"
		| "onSetInstructions"
		| "onSetRelations"
		| "onSetInfo"
		| "onSetSteps"
		| "onIgnoreVersion"
		| "onUnignoreVersion"
		| "onDeleteVersion"
		| "onDeleteUnusedVersions"
	> {
	mod: FullMod;
	/// Every installed version of the family; the status list carries only the count.
	versions: VersionView[];
	update?: UpdateReport;
	allTags: string[];
	/// The game reads load priorities from its settings file, so the section is shown.
	showPriority: boolean;
	/// Every mod of the profile, to pick the related ones from.
	mods: Mod[];
	/// The steps of the game and this mod's choices; null while the details load.
	steps: StepChoices | null;
	onRemove: (modId: string) => void;
}

/// The everyday settings of a mod: its version, what it is about, and the user's note.
export function OverviewTab({
	mod,
	versions,
	update,
	allTags,
	onSelectVersion,
	onDeleteVersion,
	onDeleteUnusedVersions,
	onSetTags,
	onSetNote,
	onSetPriority,
	onSetInstructions,
	onSetRelations,
	showPriority,
	mods,
	onSetInfo,
	onSetSteps,
	onIgnoreVersion,
	onUnignoreVersion,
	steps,
	onRemove,
}: OverviewTabProps) {
	const { t, i18n } = useTranslation();
	const author = authorOf(mod);
	const statusWord = usePageStatus();
	const installedAt = mod.installed_at;
	const source = mod.source;
	const dateText = (seconds: number | null | undefined) => (seconds == null ? null : formatDate(seconds * 1000, i18n.language));
	const countText = (value: number | null | undefined) => (value == null ? null : formatCount(value, i18n.language));

	const facts = [
		fact(t("mods.info.author"), author ?? t("common.none"), { className: "truncate" }),
		source?.uploader !== author ? fact(t("mods.info.uploader"), source?.uploader, { className: "truncate" }) : null,
		fact(t("mods.info.category"), source?.category, { className: "truncate" }),
		fact(t("mods.info.endorsements"), countText(source?.endorsements), { className: "tabular-nums" }),
		fact(t("mods.info.downloads"), countText(source?.downloads), { className: "tabular-nums" }),
		fact(t("mods.info.size"), formatSize(mod.size_kb, i18n.language), { className: "font-mono tabular-nums" }),
		fact(t("mods.col.sourceName"), source?.name, { className: "truncate" }),
		fact(t("mods.info.page"), mod.page_name ?? mod.page_id, { className: "truncate" }),
		fact(t("mods.info.part"), mod.part_name, { className: "truncate" }),
		fact(t("mods.info.partCategory"), mod.part_category),
		fact(t("mods.col.uniqueDownloads"), countText(source?.unique_downloads), { className: "tabular-nums" }),
		fact(t("mods.col.adult"), source?.adult ? t("mods.col.adultYes") : null),
		fact(t("mods.col.sourceStatus"), source?.status ? statusWord(source.status) : null),
		fact(t("mods.col.sourceCreated"), dateText(source?.created_at), { className: "tabular-nums" }),
		fact(t("mods.col.sourceUpdated"), dateText(source?.updated_at), { className: "tabular-nums" }),
		fact(t("mods.col.checked"), dateText(source?.checked_at), { className: "tabular-nums" }),
		fact(t("mods.col.sourceTags"), source?.tags.join(", ")),
		fact(t("mods.col.summary"), source?.summary),
		fact(t("mods.col.installed"), dateText(installedAt), { className: "tabular-nums" }),
		fact(t("mods.col.choices"), mod.fomod.length > 0 ? choicesText(mod.fomod) : null, { className: "truncate" }),
		fact(t("mods.col.profiles"), mod.profiles.join(", ")),
	];

	/// Where the files live on disk and the identifiers Gobbo uses. They help when something goes wrong, so they sit
	/// behind a disclosure rather than in the everyday list.
	const technicalFacts = [
		fact(t("mods.info.archive"), mod.archive ?? t("common.none"), {
			className: "truncate font-mono text-xs",
			title: mod.archive ?? undefined,
		}),
		fact(t("mods.col.sourceFile"), mod.sources.find((entry) => entry.file_id)?.file_id, { className: "tabular-nums" }),
		fact(t("mods.info.part"), mod.part_id, { className: "tabular-nums" }),
		fact(t("mods.col.folder"), mod.folder, { className: "truncate font-mono text-xs", title: mod.folder }),
		fact(t("mods.col.archiveFile"), mod.archive_file, { className: "truncate font-mono text-xs", title: mod.archive_file ?? undefined }),
		fact(t("mods.col.slot"), mod.slot, { className: "truncate font-mono text-xs" }),
		fact(t("mods.col.requirementsRead"), dateText(mod.source_sync[0]?.checked_at), { className: "tabular-nums" }),
		fact(t("mods.col.id"), mod.id, { className: "truncate font-mono text-xs", title: mod.id }),
	];

	return (
		<>
			{mod.info.description && <p className="text-muted-foreground text-sm">{mod.info.description}</p>}

			{mod.page_description && (
				<Disclosure title={t("mods.about")}>
					<p className="text-muted-foreground max-h-96 overflow-y-auto text-sm leading-relaxed wrap-break-word whitespace-pre-line">
						{mod.page_description}
					</p>
				</Disclosure>
			)}

			<VersionSection
				mod={mod}
				versions={versions}
				update={update}
				onSelectVersion={onSelectVersion}
				onDeleteVersion={onDeleteVersion}
				onDeleteUnusedVersions={onDeleteUnusedVersions}
				onIgnoreVersion={onIgnoreVersion}
				onUnignoreVersion={onUnignoreVersion}
			/>

			<FactList facts={facts} />

			{technicalFacts.some((item) => item !== null) && (
				<Disclosure title={t("common.details")}>
					<FactList facts={technicalFacts} />
				</Disclosure>
			)}

			<InfoSection
				key={`info-${mod.folder}-${mod.info.name}-${mod.info.version ?? ""}-${mod.info.author ?? ""}`}
				mod={mod}
				onSetInfo={onSetInfo}
			/>
			{steps && steps.defined.length > 0 && (
				<StepsSection
					key={`steps-${steps.before_start.join(",")}|${steps.after_end.join(",")}`}
					steps={steps}
					onSave={(beforeStart, afterEnd) => onSetSteps(mod.folder, beforeStart, afterEnd)}
				/>
			)}
			{showPriority && (
				<PrioritySection key={`priority-${mod.folder}-${mod.load_priority ?? ""}`} mod={mod} onSetPriority={onSetPriority} />
			)}
			<TagsSection mod={mod} allTags={allTags} onSetTags={onSetTags} />
			<NoteSection key={`note-${mod.folder}-${mod.notes}`} mod={mod} onSetNote={onSetNote} />
			<InstructionsSection key={`instructions-${mod.instructions}`} mod={mod} onSetInstructions={onSetInstructions} />
			<RelationsSection mod={mod} mods={mods} onSave={(relations) => onSetRelations(mod.id, relations)} />
			<section>
				<Button
					variant="ghost"
					size="sm"
					className="text-destructive hover:text-destructive justify-self-start"
					onClick={() => onRemove(mod.id)}
				>
					<Trash2 /> {t("mods.deleteMod")}
				</Button>
			</section>
		</>
	);
}

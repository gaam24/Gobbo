import { X } from "lucide-react";
import { useId, useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { RelationKind, Relations } from "@/bindings";
import { SectionHeading } from "@/components/app/content-blocks";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { FullMod, Mod } from "@/lib/mods";
import { SourceRequirements } from "@/sources/source-requirements";

/// The kinds of relation, in the order the inspector shows them.
const RELATION_KINDS = ["requires", "incompatible", "overrides"] as const satisfies ReadonlyArray<keyof Relations>;

/// The mods this mod needs, the mods it does not work with, and the mods it overrides (it stands above them, so its
/// copy of a shared file wins). Gobbo warns about them, sorts by them on request, and never blocks a deployment.
export function RelationsSection({
	mod,
	mods,
}: {
	mod: FullMod;
	mods: Mod[];
	/// The inspector still passes a whole-list save; each item below now goes through its own action instead.
	onSave?: (relations: Relations) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const { gameActions } = useWorkspace();
	/// Each edit names one target in one list. The backend merges it into its newest list, so two quick edits keep both.
	const add = (kind: RelationKind, id: string) => void gameActions.addRelation(mod.id, kind, id);
	const remove = (kind: RelationKind, id: string) => void gameActions.removeRelation(mod.id, kind, id);
	const nameOf = (id: string) => mods.find((item) => item.id === id)?.info.name ?? id;
	const others = mods.filter((item) => item.id !== mod.id);
	const relations: Relations = {
		requires: mod.requires,
		requirements: mod.requirements.map(({ source, name, from_source, group, ignored }) => ({
			source,
			name,
			from_source,
			group,
			ignored,
		})),
		incompatible: mod.incompatible,
		overrides: mod.overrides,
	};
	const listed = new Set(RELATION_KINDS.flatMap((kind) => relations[kind]));

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.relations.title")} hint={t("mods.relations.hint")} />
			<div className="grid gap-4">
				{RELATION_KINDS.map((kind) => (
					<RelationList
						key={kind}
						title={t(`mods.relations.${kind}`)}
						ids={relations[kind]}
						nameOf={nameOf}
						options={others.filter((item) => !listed.has(item.id))}
						onAdd={(id) => add(kind, id)}
						onRemove={(id) => remove(kind, id)}
					/>
				))}
				<SourceRequirements key={mod.folder} mod={mod} />
			</div>
		</section>
	);
}

/// One kind of relation: the mods it names as removable chips, and a list to add another one.
function RelationList({
	title,
	ids,
	nameOf,
	options,
	onAdd,
	onRemove,
}: {
	title: string;
	ids: string[];
	nameOf: (id: string) => string;
	options: Mod[];
	onAdd: (id: string) => void;
	onRemove: (id: string) => void;
}) {
	const { t } = useTranslation();
	const selectId = useId();
	/// The list can hold every other mod. Radix builds its items only while the select is open; keeping the element tree
	/// out of the closed render keeps the inspector cheap on a library of thousands.
	const [open, setOpen] = useState(false);
	return (
		<div className="grid gap-2">
			<span className="text-sm font-medium">{title}</span>
			{ids.length > 0 ? (
				<ul className="flex flex-wrap gap-1.5">
					{ids.map((id) => (
						<li key={id}>
							<Badge variant="secondary" className="h-auto gap-1 py-0.5 pr-1 pl-2.5 text-sm font-normal">
								{nameOf(id)}
								<Button
									variant="ghost"
									size="icon-xs"
									aria-label={t("mods.relations.remove", { name: nameOf(id) })}
									className="size-5 rounded-full p-0"
									onClick={() => onRemove(id)}
								>
									<X className="size-3.5" aria-hidden />
								</Button>
							</Badge>
						</li>
					))}
				</ul>
			) : (
				<span className="text-subtle-foreground text-xs">{t("mods.relations.empty")}</span>
			)}
			{options.length > 0 && (
				<Field>
					<FieldLabel htmlFor={selectId} className="sr-only">
						{title}
					</FieldLabel>
					<Select value="" onValueChange={(id) => id && onAdd(id)} onOpenChange={setOpen}>
						<SelectTrigger id={selectId} className="w-full">
							<SelectValue placeholder={t("mods.relations.add")} />
						</SelectTrigger>
						{open && (
							<SelectContent>
								{options.map((item) => (
									<SelectItem key={item.id} value={item.id}>
										{item.info.name}
									</SelectItem>
								))}
							</SelectContent>
						)}
					</Select>
				</Field>
			)}
		</div>
	);
}

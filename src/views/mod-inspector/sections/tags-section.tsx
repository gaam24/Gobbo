import { X } from "lucide-react";
import { useState } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { SuggestionInput } from "@/components/suggestion-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { tagSuggestions, withTag } from "@/lib/mod-tags";
import type { Mod } from "@/lib/mods";

/// Tags of the selected mod: removable chips, and a field whose list offers the game's other tags. Enter or a pick from
/// the list adds a tag.
export function TagsSection({
	mod,
	allTags,
	onSetTags,
}: {
	mod: Mod;
	allTags: string[];
	onSetTags: (modId: string, tags: string[]) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [value, setValue] = useState("");

	// A blank or repeated name changes nothing, and then the field keeps its text.
	const add = (name: string) => {
		const tags = withTag(mod.tags, name);
		if (tags === null) return;
		void onSetTags(mod.id, tags);
		setValue("");
	};

	return (
		<section className="grid gap-2">
			<SectionHeading title={t("mods.tags.title")} />
			{mod.tags.length > 0 && (
				<div className="flex flex-wrap gap-1.5">
					{mod.tags.map((tag) => (
						<Badge key={tag} variant="secondary" className="gap-1 pr-1">
							{tag}
							<Button
								variant="ghost"
								size="icon-xs"
								aria-label={t("mods.tags.remove", { tag })}
								className="size-4 rounded-full p-0"
								onClick={() =>
									void onSetTags(
										mod.id,
										mod.tags.filter((kept) => kept !== tag),
									)
								}
							>
								<X className="size-3" aria-hidden />
							</Button>
						</Badge>
					))}
				</div>
			)}
			<Field>
				<FieldLabel htmlFor="mod-tags" className="sr-only">
					{t("mods.tags.title")}
				</FieldLabel>
				<SuggestionInput
					id="mod-tags"
					value={value}
					onChange={setValue}
					onSelect={add}
					onKeyDown={(event) => {
						if (event.key === "Enter") {
							event.preventDefault();
							add(value);
						}
					}}
					suggestions={tagSuggestions(allTags, mod.tags)}
					menuLabel={t("mods.tags.existing")}
					emptyLabel={t("mods.tags.none")}
					placeholder={t("mods.tags.placeholder")}
					menuClassName="w-56"
				/>
			</Field>
		</section>
	);
}

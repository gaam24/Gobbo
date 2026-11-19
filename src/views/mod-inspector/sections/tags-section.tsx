import { X } from "lucide-react";
import { useState } from "react";
import { SectionHeading } from "@/components/app/content-blocks";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// Tags of the selected mod: removable chips, and a field with suggestions from all the game's tags.
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
	const trimmed = value.trim();

	const add = () => {
		if (!trimmed || mod.tags.includes(trimmed)) return;
		void onSetTags(mod.id, [...mod.tags, trimmed]);
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
				<Input
					id="mod-tags"
					value={value}
					placeholder={t("mods.tags.placeholder")}
					list="gobbo-mod-tags"
					spellCheck={false}
					onChange={(event) => setValue(event.currentTarget.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter") {
							event.preventDefault();
							add();
						}
					}}
				/>
			</Field>
			<datalist id="gobbo-mod-tags">
				{allTags.map((tag) => (
					<option key={tag} value={tag} aria-label={tag} />
				))}
			</datalist>
		</section>
	);
}

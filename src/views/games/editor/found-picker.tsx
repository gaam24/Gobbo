import { FolderOpen } from "lucide-react";
import { SuggestionInput } from "@/components/suggestion-input";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";

/// A path field that also lists what Gobbo found in the game folder. The list shows every entry, so a path already in the
/// field does not hide the others, and the field still takes any text. `onBrowse`, when given, fills it from the file
/// dialog. The shared list lives in `SuggestionInput`; this wrapper only adds the path styling and labels.
export function FoundPicker({
	id,
	value,
	onChange,
	found,
	menuLabel,
	placeholder,
	onBrowse,
}: {
	id: string;
	value: string;
	onChange: (value: string) => void;
	/** Every entry Gobbo found, best first. */
	found: string[];
	/** Names the list button and heads the list, for example "Programs Gobbo found". */
	menuLabel: string;
	placeholder?: string;
	/** Opens the file dialog. The caller turns the answer into a value; omitted when no dialog is offered. */
	onBrowse?: () => void;
}) {
	const { t } = useTranslation();

	return (
		<SuggestionInput
			id={id}
			value={value}
			onChange={onChange}
			suggestions={found}
			menuLabel={menuLabel}
			emptyLabel={t("editor.quick.foundNone")}
			placeholder={placeholder}
			inputClassName="font-mono text-xs"
			menuClassName="w-[min(28rem,calc(100vw-2rem))] font-mono text-xs"
		>
			{onBrowse && (
				<Button type="button" variant="outline" onClick={onBrowse}>
					<FolderOpen /> {t("editor.quick.browse")}
				</Button>
			)}
		</SuggestionInput>
	);
}

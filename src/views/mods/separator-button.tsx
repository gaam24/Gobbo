import { SplitSquareVertical } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useTranslation } from "@/i18n";

/// Adds a separator above the focused mod: a named header that groups the mods below it.
export function SeparatorButton({ disabled, onAdd }: { disabled: boolean; onAdd: (name: string) => void | Promise<void> }) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const [name, setName] = useState("");
	const add = () => {
		const trimmed = name.trim();
		if (!trimmed) return;
		setOpen(false);
		setName("");
		void onAdd(trimmed);
	};

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<Button variant="ghost" size="icon" disabled={disabled} aria-label={t("separators.add")} title={t("separators.addHint")}>
					<SplitSquareVertical />
				</Button>
			</PopoverTrigger>
			<PopoverContent className="grid w-72 gap-2 p-3" align="end">
				<Field>
					<FieldLabel htmlFor="separator-name">{t("separators.name")}</FieldLabel>
					<Input
						id="separator-name"
						autoFocus
						value={name}
						maxLength={60}
						placeholder={t("separators.placeholder")}
						onChange={(event) => setName(event.currentTarget.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") add();
						}}
					/>
				</Field>
				<Button size="sm" className="justify-self-end" disabled={!name.trim()} onClick={add}>
					{t("separators.add")}
				</Button>
			</PopoverContent>
		</Popover>
	);
}

import { cn } from "cn";
import { ChevronDown } from "lucide-react";
import type { KeyboardEvent, ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";

/// A text field with a button beside it that opens a list of suggestions. The list always shows every entry: it is not a
/// filter of the text, so a value already in the field does not hide the others. The field still takes any text. Picking
/// an entry calls `onSelect`, or `onChange` when there is none, so a caller can put the entry in the field.
export function SuggestionInput({
	id,
	value,
	onChange,
	onSelect,
	onKeyDown,
	suggestions,
	menuLabel,
	emptyLabel,
	placeholder,
	inputClassName,
	menuClassName,
	children,
}: {
	id: string;
	value: string;
	onChange: (value: string) => void;
	/** Takes an entry picked from the list. Defaults to `onChange`, which puts the entry in the field. */
	onSelect?: (entry: string) => void;
	onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
	/** Every entry the list offers, in the order shown. */
	suggestions: readonly string[];
	/** Names the list button and heads the list. */
	menuLabel: string;
	/** Shown in the list when there are no entries. Without it an empty list shows only its heading. */
	emptyLabel?: string;
	placeholder?: string;
	/** Extra classes for the field, after its layout classes. */
	inputClassName?: string;
	/** Extra classes for the list, after its defaults. */
	menuClassName?: string;
	/** Controls drawn after the list button in the same row, such as a browse button. */
	children?: ReactNode;
}) {
	const pick = onSelect ?? onChange;

	return (
		<div className="flex gap-2">
			<Input
				id={id}
				className={cn("min-w-0 flex-1", inputClassName)}
				value={value}
				placeholder={placeholder}
				spellCheck={false}
				onChange={(event) => onChange(event.currentTarget.value)}
				onKeyDown={onKeyDown}
			/>
			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button type="button" variant="outline" size="icon" aria-label={menuLabel}>
						<ChevronDown />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" className={menuClassName}>
					<DropdownMenuLabel className="font-sans">{menuLabel}</DropdownMenuLabel>
					{suggestions.length === 0 && emptyLabel && <DropdownMenuItem disabled>{emptyLabel}</DropdownMenuItem>}
					{suggestions.map((entry) => (
						<DropdownMenuItem key={entry} onSelect={() => pick(entry)}>
							{entry}
						</DropdownMenuItem>
					))}
				</DropdownMenuContent>
			</DropdownMenu>
			{children}
		</div>
	);
}

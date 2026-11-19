import { X } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";

/// A labelled control in the editor. The label and the hint sit above it, because the advanced section is a form, not
/// a list of settings rows.
export function EditorField({
	id,
	label,
	hint,
	children,
	className,
}: {
	id?: string;
	label: ReactNode;
	hint?: ReactNode;
	children: ReactNode;
	className?: string;
}) {
	return (
		<Field className={className}>
			<FieldLabel htmlFor={id}>{label}</FieldLabel>
			{children}
			{hint && <FieldDescription>{hint}</FieldDescription>}
		</Field>
	);
}

/// A small text input for one string.
export function TextInput({
	id,
	value,
	onChange,
	placeholder,
	className,
}: {
	id?: string;
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	className?: string;
}) {
	return (
		<Input
			id={id}
			className={className}
			value={value}
			placeholder={placeholder}
			spellCheck={false}
			onChange={(event) => onChange(event.currentTarget.value)}
		/>
	);
}

/// A switch with its label and hint on one line.
export function SwitchField({
	id,
	label,
	hint,
	checked,
	onChange,
}: {
	id?: string;
	label: ReactNode;
	hint?: ReactNode;
	checked: boolean;
	onChange: (checked: boolean) => void;
}) {
	return (
		<Field orientation="horizontal" className="items-start">
			<div className="grid flex-1 gap-1">
				<FieldLabel htmlFor={id}>{label}</FieldLabel>
				{hint && <FieldDescription>{hint}</FieldDescription>}
			</div>
			<Switch id={id} checked={checked} onCheckedChange={onChange} />
		</Field>
	);
}

/// A list edited as one item per line. Paths and arguments are easier to read this way than in a comma-separated line.
export function LinesField({
	id,
	label,
	hint,
	value,
	onChange,
	placeholder,
	rows = 2,
	mono = true,
}: {
	id?: string;
	label: ReactNode;
	hint?: ReactNode;
	value: string[] | undefined;
	onChange: (value: string[]) => void;
	placeholder?: string;
	rows?: number;
	mono?: boolean;
}) {
	const { t } = useTranslation();
	return (
		<EditorField id={id} label={label} hint={hint ?? t("editor.fields.onePerLine")}>
			<Textarea
				id={id}
				rows={rows}
				className={mono ? "font-mono text-xs" : undefined}
				value={(value ?? []).join("\n")}
				placeholder={placeholder}
				spellCheck={false}
				onChange={(event) => onChange(event.currentTarget.value.split("\n"))}
			/>
		</EditorField>
	);
}

/// The frame of one repeatable row in the advanced form.
export function RepeatRow({ title, onRemove, children }: { title: ReactNode; onRemove: () => void; children: ReactNode }) {
	const { t } = useTranslation();
	return (
		<div className="bg-muted/30 grid gap-4 rounded-lg border p-4">
			<div className="flex items-center justify-between gap-3">
				<span className="min-w-0 truncate text-sm font-medium">{title}</span>
				<Button type="button" variant="ghost" size="icon-sm" aria-label={t("editor.removeRow")} onClick={onRemove}>
					<X />
				</Button>
			</div>
			{children}
		</div>
	);
}

/// A button that appends an empty row, used under every repeatable list.
export function AddRowButton({ label, onClick }: { label: string; onClick: () => void }) {
	return (
		<Button type="button" variant="outline" size="sm" className="w-fit" onClick={onClick}>
			{label}
		</Button>
	);
}

/// The heading, the add button and the list of one repeatable table. Kept in one component so every table has the same
/// shape and the advanced form does not repeat its wrapper.
export function RepeatableSection({
	title,
	hint,
	addLabel,
	onAdd,
	divided = false,
	children,
}: {
	title: string;
	hint: string;
	addLabel: string;
	onAdd: () => void;
	divided?: boolean;
	children: ReactNode;
}) {
	return (
		<div className={divided ? "border-t pt-6" : undefined}>
			<div className="mb-4 flex items-center justify-between gap-3">
				<div>
					<h3 className="text-sm font-semibold">{title}</h3>
					<p className="text-muted-foreground text-xs">{hint}</p>
				</div>
				<AddRowButton label={addLabel} onClick={onAdd} />
			</div>
			<div className="grid gap-3">{children}</div>
		</div>
	);
}

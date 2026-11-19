import { cn } from "cn";
import type { ReactNode } from "react";
import { HelpHint } from "@/components/app/help-hint";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";

/// Settings row: title and hint on the left, the control on the right. Rows are separated by a line, not cards.
export function SettingRow({
	title,
	htmlFor,
	hint,
	help,
	action,
	className,
	children,
}: {
	title: string;
	htmlFor?: string;
	hint?: string;
	/// A longer explanation for the question mark next to the title. Keep `hint` for short, essential text.
	help?: string;
	/// A control that belongs to the title, such as the link to the page where the value is made. It sits under the title.
	action?: ReactNode;
	className?: string;
	children: ReactNode;
}) {
	return (
		<div className="@container/field-group">
			<Field orientation="responsive" className={cn("gap-x-10 gap-y-3 border-t py-6", className)}>
				<FieldContent className="gap-1 @md/field-group:flex-none @md/field-group:basis-56">
					<div className="flex items-center gap-1">
						<FieldLabel htmlFor={htmlFor} className="text-sm font-medium">
							{title}
						</FieldLabel>
						{help && <HelpHint>{help}</HelpHint>}
					</div>
					{hint && <FieldDescription>{hint}</FieldDescription>}
					{action}
				</FieldContent>
				<div className="min-w-0 @md/field-group:flex-1">{children}</div>
			</Field>
		</div>
	);
}

/// A section of a page: a heading, then its rows or its one block. `hint` is the single line that must show under the
/// heading; `help` holds the longer explanation behind the question mark, so the heading is never repeated in a row.
export function Section({
	id,
	title,
	hint,
	help,
	children,
	danger = false,
}: {
	id: string;
	title: string;
	hint?: string;
	help?: string;
	children: ReactNode;
	danger?: boolean;
}) {
	return (
		<section id={id} aria-labelledby={`${id}-title`} className="scroll-mt-6">
			<div className="flex items-center gap-1">
				<h2 id={`${id}-title`} className={danger ? "text-destructive text-base font-semibold" : "text-base font-semibold"}>
					{title}
				</h2>
				{help && <HelpHint>{help}</HelpHint>}
			</div>
			{hint && <p className="text-muted-foreground mt-1 text-sm text-pretty">{hint}</p>}
			{children}
		</section>
	);
}

/// The one block of a section that has no rows of its own. It takes the same top line as a row, so the sections line up.
export function SectionBlock({ children }: { children: ReactNode }) {
	return <div className="border-t py-6">{children}</div>;
}

/// A named group inside a section, such as one provider with its key and link options. The title sits above the rows;
/// each row draws its own top line, so the group adds no border of its own.
export function SettingsGroup({ title, className, children }: { title: string; className?: string; children: ReactNode }) {
	return (
		<section className={cn("grid", className)}>
			<h3 className="text-sm font-semibold">{title}</h3>
			<div className="mt-2">{children}</div>
		</section>
	);
}

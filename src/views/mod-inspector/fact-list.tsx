import { Fragment, type ReactNode } from "react";

/// One line of a facts list: its label, and its value with the classes and title the value needs.
export interface Fact {
	label: string;
	value: ReactNode;
	className?: string;
	title?: string;
}

/// A line of the facts list. A value that is missing gives no line: a fact the provider does not give is not shown.
export function fact(label: string, value: ReactNode, details?: Pick<Fact, "className" | "title">): Fact | null {
	if (value == null || value === false || value === "") return null;
	return { label, value, ...details };
}

/// A definition list of facts, a label beside its value. A missing fact (null) gives no line.
export function FactList({ facts }: { facts: Array<Fact | null> }) {
	return (
		<dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
			{facts
				.filter((item): item is Fact => item !== null)
				.map(({ label, value, className, title }) => (
					<Fragment key={label}>
						<dt className="text-muted-foreground">{label}</dt>
						<dd className={className} title={title}>
							{value}
						</dd>
					</Fragment>
				))}
		</dl>
	);
}

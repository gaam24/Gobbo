import type { ReactNode } from "react";

/// One numbered step of the quick form. The number is visual; the heading carries the meaning for a screen reader.
export function QuickSection({ number, title, hint, children }: { number: number; title: string; hint: string; children: ReactNode }) {
	return (
		<section className="grid gap-4 border-t pt-6">
			<div className="flex items-start gap-3">
				<span className="bg-primary text-primary-foreground flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold">
					{number}
				</span>
				<div className="grid gap-1">
					<h2 className="text-base font-semibold">{title}</h2>
					<p className="text-muted-foreground text-sm text-pretty">{hint}</p>
				</div>
			</div>
			{children}
		</section>
	);
}

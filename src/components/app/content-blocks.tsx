import { cn } from "cn";
import { ChevronRight } from "lucide-react";
import { type ComponentProps, type ReactNode, useState } from "react";
import { HelpHint } from "@/components/app/help-hint";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Item } from "@/components/ui/item";

/// The building blocks inside a dialog or the details panel, so every list, heading and fold there looks the same.

/// A heading over a block of a dialog or the details panel: the name, how many items it holds and its actions.
export function SectionHeading({
	title,
	hint,
	count,
	actions,
	className,
}: {
	title: ReactNode;
	/// The explanation behind the question mark, right next to the title.
	hint?: ReactNode;
	count?: number;
	actions?: ReactNode;
	className?: string;
}) {
	return (
		<div className={cn("flex min-h-7 items-center gap-2", className)}>
			<h3 className="min-w-0 truncate text-sm font-semibold">{title}</h3>
			{hint && <HelpHint>{hint}</HelpHint>}
			{count !== undefined && <span className="text-subtle-foreground font-mono text-xs tabular-nums">{count}</span>}
			{actions && <div className="ml-auto flex shrink-0 items-center gap-1">{actions}</div>}
		</div>
	);
}

/// A list inside a dialog or a panel: one bordered box with a divider between rows. `scroll` caps a long list so the
/// dialog's actions stay in view; `tall` gives a list that is the dialog's main content more room.
export function BoxList({
	scroll = false,
	tall = false,
	ordered = false,
	className,
	children,
}: {
	scroll?: boolean;
	tall?: boolean;
	/// An `ol`, for a list whose order means something (priorities, steps).
	ordered?: boolean;
	className?: string;
	children: ReactNode;
}) {
	const List = ordered ? "ol" : "ul";
	return (
		<List className={cn("rounded-lg border", scroll && "overflow-y-auto", scroll && (tall ? "max-h-[45vh]" : "max-h-64"), className)}>
			{children}
		</List>
	);
}

/// One row of a `BoxList`, an `li`. The row draws the divider itself: `Item` sets its own border, which would hide a
/// `divide-y` on the list. Put `ItemMedia`, `ItemContent` and `ItemActions` inside it.
export function BoxListRow({ className, children, ...props }: ComponentProps<typeof Item>) {
	return (
		<Item asChild size="sm" className={cn("border-border rounded-none border-0 border-b px-3 py-2 last:border-b-0", className)} {...props}>
			<li>{children}</li>
		</Item>
	);
}

/// A part that opens and closes: a chevron, the title and a count. It replaces the browser's `<details>`, whose marker
/// differs from every other fold in the app. A closed part does not mount its children, so a long list costs nothing.
export function Disclosure({
	title,
	count,
	defaultOpen = false,
	open,
	onOpenChange,
	className,
	children,
}: {
	title: ReactNode;
	count?: number;
	defaultOpen?: boolean;
	/// Controlled state, when the caller keeps it (for example in a saved preference).
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
	className?: string;
	children: ReactNode;
}) {
	const [own, setOwn] = useState(defaultOpen);
	const shown = open ?? own;
	return (
		<Collapsible
			open={shown}
			onOpenChange={(next) => {
				setOwn(next);
				onOpenChange?.(next);
			}}
			className={cn("grid gap-2", className)}
		>
			<CollapsibleTrigger className="focus-visible:ring-ring group/disclosure flex min-h-7 items-center gap-1.5 self-start rounded-sm text-left text-sm font-semibold outline-none focus-visible:ring-2">
				<ChevronRight
					className="text-muted-foreground size-4 shrink-0 transition-transform duration-150 group-data-[state=open]/disclosure:rotate-90 motion-reduce:transition-none"
					aria-hidden
				/>
				<span className="min-w-0">{title}</span>
				{count !== undefined && <span className="text-subtle-foreground font-mono text-xs font-normal tabular-nums">{count}</span>}
			</CollapsibleTrigger>
			<CollapsibleContent className="grid gap-2">{children}</CollapsibleContent>
		</Collapsible>
	);
}

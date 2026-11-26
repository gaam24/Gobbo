import { cn } from "cn";
import { ChevronRight, type LucideIcon } from "lucide-react";
import { type ComponentProps, type ReactNode, useState } from "react";
import { HelpHint } from "@/components/app/help-hint";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Item, ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { Spinner } from "@/components/ui/spinner";

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

/// One outcome in a `BoxList`: an icon, what happens and, in one sentence, what that means for the user. It is the
/// pattern of the closing dialog, for dialogs that report instead of ask (an apply preview, a check, a delete).
/// The icon colour carries the outcome (`primary` for the good result, `warning` for what needs a look, `destructive`
/// for what is lost); the text always says the same thing, so colour never works alone.
export function OutcomeRow({
	icon: Icon,
	title,
	detail,
	tone,
	action,
}: {
	icon: LucideIcon;
	title: ReactNode;
	detail?: ReactNode;
	tone?: "primary" | "warning" | "destructive";
	/// The button that acts on this outcome, such as "Show" or "Turn on".
	action?: ReactNode;
}) {
	return (
		<BoxListRow className="items-start py-2.5">
			<ItemMedia className="mt-0.5">
				<Icon
					className={cn(
						"text-muted-foreground size-4",
						tone === "primary" && "text-primary",
						tone === "warning" && "text-draft",
						tone === "destructive" && "text-destructive",
					)}
					aria-hidden
				/>
			</ItemMedia>
			<ItemContent className="min-w-0 gap-0.5">
				<ItemTitle className="w-full font-normal whitespace-normal">{title}</ItemTitle>
				{detail && <ItemDescription className="text-xs">{detail}</ItemDescription>}
			</ItemContent>
			{action && <ItemActions>{action}</ItemActions>}
		</BoxListRow>
	);
}

/// The one line a dialog or a panel shows instead of its content: while it loads (with a spinner), when it has nothing to
/// show, or when reading it failed. Every such line looks and is announced the same way, so a loading state never reads
/// as an empty one and a failure is always read out.
export function StatusLine({
	tone = "muted",
	className,
	children,
}: {
	tone?: "loading" | "muted" | "error";
	className?: string;
	children: ReactNode;
}) {
	if (tone === "loading") {
		return (
			<p role="status" className={cn("text-muted-foreground flex items-center gap-2 text-sm", className)}>
				<Spinner aria-hidden /> {children}
			</p>
		);
	}

	return (
		<p
			role={tone === "error" ? "alert" : undefined}
			className={cn("text-sm", tone === "error" ? "text-destructive" : "text-muted-foreground", className)}
		>
			{children}
		</p>
	);
}

import { cn } from "cn";
import { ArrowLeft, type LucideIcon } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { useOptionalNavigation } from "@/app/contexts/navigation-context";
import { HelpHint } from "@/components/app/help-hint";
import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Item } from "@/components/ui/item";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTranslation } from "@/i18n";

/// The building blocks every screen shares, taken from the mod list: a 48 px header with the title and its actions, an
/// optional toolbar, a scrolling body, edge-to-edge rows with group headers, and one empty state. A screen composes
/// these instead of drawing its own header, so every page has the same gutters, heights and type.

/// The views that are roots of the history; every other screen offers the way back in its header.
const ROOTS = new Set(["mods", "games"]);

/// The header of a screen: the way back, the title with its explanation behind a question mark, a short meta text and
/// the screen's actions at the right.
export function PageHeader({
	title,
	hint,
	meta,
	actions,
	children,
}: {
	title: ReactNode;
	/// Why the screen exists or how it behaves. It sits behind the question mark, not in a paragraph under the title.
	hint?: ReactNode;
	/// A short fact next to the title, such as a count or a store id.
	meta?: ReactNode;
	actions?: ReactNode;
	/// Extra controls right after the title, such as the profile switcher of the mod list.
	children?: ReactNode;
}) {
	const { t } = useTranslation();
	const navigation = useOptionalNavigation();
	return (
		<header className="flex min-h-12 shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b px-6 py-2">
			<div className="flex max-w-full min-w-0 shrink items-center gap-3">
				{navigation?.canGoBack && !ROOTS.has(navigation.view) && (
					<Button
						variant="ghost"
						size="icon-sm"
						className="-ml-2"
						aria-label={t("common.back")}
						title={t("common.back")}
						onClick={navigation.back}
					>
						<ArrowLeft />
					</Button>
				)}
				<h1 className="min-w-0 truncate text-base font-semibold">{title}</h1>
				{hint && <HelpHint>{hint}</HelpHint>}
				{meta && <span className="text-muted-foreground shrink-0 text-xs">{meta}</span>}
				{children}
			</div>
			{actions && <div className="ml-auto flex max-w-full min-w-0 flex-wrap items-center justify-end gap-2">{actions}</div>}
		</header>
	);
}

/// The row under the header with the search, the filters and the list actions.
export function PageToolbar({ className, ...props }: ComponentProps<"div">) {
	return <div className={cn("flex shrink-0 flex-wrap items-center gap-2 border-b px-6 py-3", className)} {...props} />;
}

/// The scrolling part of a screen. A list fills it edge to edge; `padded` gives a form or prose page the page gutters.
export function PageBody({ padded = false, className, ...props }: ComponentProps<"div"> & { padded?: boolean }) {
	return <div className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto", padded && "px-6 pt-6 pb-8", className)} {...props} />;
}

/// A heading over a group of rows, drawn like a separator of the mod list: the name and the number of rows under it.
export function ListGroupHeader({ title, count, actions }: { title: ReactNode; count?: number; actions?: ReactNode }) {
	return (
		<div className="bg-card flex h-10 shrink-0 items-center gap-2 border-b px-6">
			<h2 className="min-w-0 truncate text-sm font-semibold">{title}</h2>
			{count !== undefined && <span className="text-subtle-foreground font-mono text-xs tabular-nums">{count}</span>}
			{actions && <div className="ml-auto flex items-center gap-1">{actions}</div>}
		</div>
	);
}

/// One row of a page list: the full width, a bottom border and the mod list's row height. Use `asChild` with an `li`.
export function ListRow({ className, ...props }: ComponentProps<typeof Item>) {
	return <Item className={cn("border-border min-h-14 rounded-none border-0 border-b px-6 py-2", className)} {...props} />;
}

/// The empty state of a screen or a list, centred in the space it has.
export function PageEmpty({
	icon: Icon,
	title,
	description,
	action,
}: {
	icon: LucideIcon;
	title: ReactNode;
	description?: ReactNode;
	action?: ReactNode;
}) {
	return (
		<Empty className="py-12">
			<EmptyHeader>
				<EmptyMedia variant="icon">
					<Icon />
				</EmptyMedia>
				<EmptyTitle>{title}</EmptyTitle>
				{description && <EmptyDescription>{description}</EmptyDescription>}
			</EmptyHeader>
			{action && <EmptyContent>{action}</EmptyContent>}
		</Empty>
	);
}

/// One section of a settings screen.
export interface SettingsSection<Id extends string> {
	id: Id;
	label: string;
	icon: LucideIcon;
	/// A destructive section, such as removing the game. It is drawn in the destructive colour and belongs last.
	danger?: boolean;
	content: ReactNode;
}

/// A settings screen: the section list on the left and the chosen section in a readable column beside it. Both the app
/// and the game settings use it, so the two screens look and move the same way.
export function SettingsLayout<Id extends string>({
	label,
	sections,
	value,
	onValueChange,
}: {
	/// The accessible name of the section list.
	label: string;
	sections: Array<SettingsSection<Id>>;
	value: Id;
	onValueChange: (id: Id) => void;
}) {
	return (
		<Tabs
			value={value}
			onValueChange={(next) => {
				const found = sections.find((section) => section.id === next);
				if (found) onValueChange(found.id);
			}}
			orientation="vertical"
			className="flex flex-col gap-8 md:flex-row"
		>
			<TabsList aria-label={label} className="w-full shrink-0 items-stretch justify-start p-1 md:sticky md:top-0 md:w-56 md:self-start">
				{sections.map((section) => {
					const Icon = section.icon;
					return (
						<TabsTrigger
							key={section.id}
							value={section.id}
							className={cn("justify-start", section.danger && "text-destructive data-active:text-destructive")}
						>
							<Icon /> {section.label}
						</TabsTrigger>
					);
				})}
			</TabsList>
			<div className="max-w-3xl min-w-0 flex-1">
				{sections.map((section) => (
					<TabsContent key={section.id} value={section.id}>
						{section.content}
					</TabsContent>
				))}
			</div>
		</Tabs>
	);
}

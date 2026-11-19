import { lazy, type ReactNode, Suspense } from "react";

const ConsoleSplit = lazy(async () => {
	const { ConsoleSplit: component } = await import("@/views/console-split");
	return { default: component };
});

/// Whether an element is inside a dialog, a menu or a select, whose keys are their own.
export function inOverlay(element: HTMLElement | null): boolean {
	return Boolean(element?.closest("[role='dialog'], [role='alertdialog'], [role='menu'], [role='listbox'], [role='combobox']"));
}

/// The console's starting share of the main column, in percent.
export const CONSOLE_SHARE = 36;

/// A plain flex container until the console opens; only then does the lazy splitter mount.
export function ModsSplit({
	consoleOpen,
	defaultShare,
	onShare,
	console,
	children,
}: {
	consoleOpen: boolean;
	defaultShare: number;
	onShare: (share: number) => void;
	console: ReactNode;
	children: ReactNode;
}) {
	if (!consoleOpen) return <div className="flex min-h-0 flex-1 flex-col">{children}</div>;

	return (
		<Suspense fallback={<div className="flex min-h-0 flex-1 flex-col">{children}</div>}>
			<ConsoleSplit defaultShare={defaultShare} resetShare={CONSOLE_SHARE} onShare={onShare} main={children} console={console} />
		</Suspense>
	);
}

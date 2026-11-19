import type { ReactNode } from "react";
import { PageBody, PageHeader, PageToolbar } from "@/components/app/page-layout";

/// A screen of the app, laid out like the mod list: the header, an optional toolbar, then the body, which scrolls on its
/// own. A list fills the body edge to edge with `ListRow`s; a form or prose page asks for the page gutters with `padded`.
export function PageFrame({
	title,
	hint,
	meta,
	action,
	toolbar,
	padded = false,
	children,
}: {
	title: string;
	/// Why the screen exists or how it behaves, behind the question mark next to the title.
	hint?: ReactNode;
	meta?: ReactNode;
	/// The screen's actions at the top right, such as the button that clears the page.
	action?: ReactNode;
	/// The search and filters under the header.
	toolbar?: ReactNode;
	padded?: boolean;
	children: ReactNode;
}) {
	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col">
			<PageHeader title={title} hint={hint} meta={meta} actions={action} />
			{toolbar && <PageToolbar>{toolbar}</PageToolbar>}
			<PageBody padded={padded}>{children}</PageBody>
		</div>
	);
}

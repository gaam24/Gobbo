import { type PointerEvent as ReactPointerEvent, useRef } from "react";

/// The left edge of a panel on the right, placed right before it: drag it, or focus it and use the arrow keys (Shift for
/// bigger steps). It sits on the panel's border and takes no room of its own.
export function PanelHandle({
	label,
	hint,
	width,
	min,
	max,
	onResize,
	onReset,
}: {
	/// The accessible name, such as "Resize the details panel".
	label: string;
	/// Shown on hover: how to use the handle.
	hint: string;
	/// The width the user set, in pixels; `null` while the panel has its automatic width.
	width: number | null;
	min: number;
	max: number;
	onResize: (width: number) => void;
	/// Brings the automatic width back: a double click, or Enter on the focused handle.
	onReset: () => void;
}) {
	/// Where the drag started: the pointer position, the width then, the panel it resizes and the last width it showed.
	/// Empty while no button is held.
	const drag = useRef<{
		x: number;
		width: number;
		panel: HTMLElement | null;
		previous: string;
		last: number;
		moved: boolean;
	} | null>(null);
	const clamped = (value: number) => Math.round(Math.min(max, Math.max(min, value)));
	/// The panel's width on screen, also while it is automatic.
	const current = (handle: HTMLElement) => handle.nextElementSibling?.getBoundingClientRect().width ?? width ?? min;

	/// The drag writes the width to the panel's CSS variable directly, so moving the pointer does not re-render the whole
	/// window and write the preference on every event. One commit follows the release.
	const endDrag = (event: ReactPointerEvent<HTMLDivElement>, commit: boolean) => {
		const start = drag.current;
		drag.current = null;
		if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
		if (start?.panel) {
			// Put back the value React rendered. On a commit the state below writes the new one; on a cancel the panel
			// keeps the width it had before the drag.
			if (start.previous) start.panel.style.setProperty("--inspector-width", start.previous);
			else start.panel.style.removeProperty("--inspector-width");
		}
		if (commit && start?.moved) onResize(start.last);
	};

	return (
		// biome-ignore lint/a11y/useSemanticElements: a focusable panel divider is the WAI-ARIA separator pattern, and no native element provides it
		<div
			role="separator"
			tabIndex={0}
			aria-orientation="vertical"
			aria-label={label}
			aria-valuemin={min}
			aria-valuemax={max}
			aria-valuenow={width ?? undefined}
			title={hint}
			className="group focus-visible:ring-ring relative z-20 -mx-1.5 flex w-3 shrink-0 cursor-col-resize touch-none justify-center outline-none focus-visible:ring-2"
			onPointerDown={(event) => {
				event.currentTarget.setPointerCapture(event.pointerId);
				const panel = event.currentTarget.nextElementSibling;
				const startWidth = current(event.currentTarget);
				drag.current = {
					x: event.clientX,
					width: startWidth,
					panel: panel instanceof HTMLElement ? panel : null,
					previous: panel instanceof HTMLElement ? panel.style.getPropertyValue("--inspector-width") : "",
					last: clamped(startWidth),
					moved: false,
				};
			}}
			onPointerMove={(event) => {
				const start = drag.current;
				// The panel is on the right: moving left makes it wider.
				if (!start) return;
				start.last = clamped(start.width - (event.clientX - start.x));
				start.moved = true;
				start.panel?.style.setProperty("--inspector-width", `${start.last}px`);
			}}
			onPointerUp={(event) => endDrag(event, true)}
			onPointerCancel={(event) => endDrag(event, false)}
			onDoubleClick={onReset}
			onKeyDown={(event) => {
				const step = event.shiftKey ? 64 : 16;
				const now = current(event.currentTarget);
				const keys: Record<string, number> = { ArrowLeft: now + step, ArrowRight: now - step, Home: max, End: min };
				const next = keys[event.key];
				if (event.key === "Enter") onReset();
				else if (next !== undefined) onResize(clamped(next));
				else return;
				event.preventDefault();
			}}
		>
			<span
				aria-hidden
				className="group-hover:bg-foreground2 group-focus-visible:bg-foreground2 group-active:bg-foreground h-full w-px transition-colors"
			/>
		</div>
	);
}

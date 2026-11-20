import { type PointerEvent as ReactPointerEvent, useRef } from "react";
import { useTranslation } from "@/i18n";

/// The width of the header cell that holds a handle. Measured, so a column that fills the space still has a width to start from.
function cellWidth(handle: HTMLElement): number {
	return handle.parentElement?.getBoundingClientRect().width ?? 0;
}

/// The right edge of a header cell. Drag it with the mouse, or focus it and use the arrow keys (Shift for bigger steps).
/// A double click fits the column to its content.
export function ColumnHandle({
	name,
	width,
	min,
	onResize,
	onFit,
}: {
	/// The column's name, for the screen reader.
	name: string;
	/// The width the user set, if any. A column that still fills the space has none.
	width?: number;
	min: number;
	onResize: (width: number) => void;
	/// Fits the column to its content: a double click, or Enter on the focused handle.
	onFit: () => void;
}) {
	const { t } = useTranslation();
	/// Where the drag started: the pointer position and the cell width then. Empty while no button is held.
	const drag = useRef<{ x: number; width: number } | null>(null);

	const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
		drag.current = null;

		if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
	};

	return (
		// biome-ignore lint/a11y/useSemanticElements: a focusable column divider is the WAI-ARIA separator pattern, and no native element provides it
		<div
			role="separator"
			tabIndex={0}
			aria-orientation="vertical"
			aria-label={t("mods.col.resize", { name })}
			aria-valuemin={min}
			aria-valuenow={width}
			title={t("mods.col.resizeHint")}
			className="group focus-visible:ring-ring absolute inset-y-2 -right-3.5 z-10 flex w-3 cursor-col-resize touch-none justify-center outline-none focus-visible:ring-2"
			onPointerDown={(event) => {
				// The edge belongs to the resize, so the header under it does not start a drag.
				event.stopPropagation();
				event.currentTarget.setPointerCapture(event.pointerId);
				drag.current = { x: event.clientX, width: cellWidth(event.currentTarget) };
			}}
			onPointerMove={(event) => {
				const start = drag.current;

				if (start) onResize(Math.max(min, start.width + event.clientX - start.x));
			}}
			onPointerUp={endDrag}
			onPointerCancel={endDrag}
			onDoubleClick={onFit}
			onKeyDown={(event) => {
				if (event.key === "Enter") {
					event.preventDefault();
					onFit();
					return;
				}

				if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
				event.preventDefault();
				const step = event.shiftKey ? 32 : 8;
				const current = cellWidth(event.currentTarget);
				onResize(Math.max(min, current + (event.key === "ArrowRight" ? step : -step)));
			}}
		>
			<span aria-hidden className="bg-line group-hover:bg-foreground2 group-focus-visible:bg-foreground2 h-full w-px" />
		</div>
	);
}

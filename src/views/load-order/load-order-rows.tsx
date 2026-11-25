import { ArrowDown, ArrowUp, Plus, X } from "lucide-react";
import { type RefObject, useRef } from "react";

import type { LoadOrderRange } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { numberAt } from "@/lib/load-order";

/// A mod as the load order window lists it: its id, its name and whether the profile turns it on.
export interface OrderRowMod {
	id: string;
	name: string;
	enabled: boolean;
}

/// The controls that take the focus after a row moved: the one in the direction of the move first, then the other.
const UP_FIRST = ["up", "down", "remove"] as const;
const DOWN_FIRST = ["down", "up", "remove"] as const;

/// The mods in the order, one row each with the number the game gets and the buttons that move or drop it. While `busy`
/// (a save runs) every button is off, so no edit is lost to the save.
export function OrderedRows({
	mods,
	range,
	busy,
	onMove,
	onRemove,
}: {
	mods: OrderRowMod[];
	range: LoadOrderRange;
	busy: boolean;
	onMove: (index: number, step: -1 | 1) => void;
	onRemove: (id: string) => void;
}) {
	const { t } = useTranslation();
	const box = useRef<HTMLDivElement>(null);

	const move = (index: number, step: -1 | 1) => {
		onMove(index, step);
		restoreFocus(box, index + step, step < 0 ? UP_FIRST : DOWN_FIRST);
	};
	const remove = (index: number, id: string) => {
		onRemove(id);
		restoreFocus(box, index, ["remove"]);
	};

	return (
		<div ref={box}>
			<BoxList scroll tall ordered>
				{mods.map((mod, index) => (
					<BoxListRow key={mod.id}>
						<RowLabel mod={mod} number={numberAt(range, index)} />
						<div className="flex shrink-0 items-center gap-1">
							<Button
								variant="ghost"
								size="icon-sm"
								data-action="up"
								aria-label={t("loadOrder.moveUp", { name: mod.name })}
								disabled={busy || index === 0}
								onClick={() => move(index, -1)}
							>
								<ArrowUp aria-hidden />
							</Button>
							<Button
								variant="ghost"
								size="icon-sm"
								data-action="down"
								aria-label={t("loadOrder.moveDown", { name: mod.name })}
								disabled={busy || index === mods.length - 1}
								onClick={() => move(index, 1)}
							>
								<ArrowDown aria-hidden />
							</Button>
							<Button
								variant="ghost"
								size="icon-sm"
								data-action="remove"
								aria-label={t("loadOrder.remove", { name: mod.name })}
								disabled={busy}
								onClick={() => remove(index, mod.id)}
							>
								<X aria-hidden />
							</Button>
						</div>
					</BoxListRow>
				))}
			</BoxList>
		</div>
	);
}

/// The mods of the profile that are not in the order yet. Each one can be added at the end of the order. While `busy`
/// the add buttons are off.
export function AvailableRows({ mods, busy, onAdd }: { mods: OrderRowMod[]; busy: boolean; onAdd: (id: string) => void }) {
	const { t } = useTranslation();
	const box = useRef<HTMLDivElement>(null);

	return (
		<div ref={box}>
			<BoxList scroll tall>
				{mods.map((mod, index) => (
					<BoxListRow key={mod.id}>
						<RowLabel mod={mod} />
						<Button
							variant="outline"
							size="sm"
							data-action="add"
							aria-label={t("loadOrder.add", { name: mod.name })}
							disabled={busy}
							onClick={() => {
								onAdd(mod.id);
								restoreFocus(box, index, ["add"]);
							}}
						>
							<Plus aria-hidden />
							{t("loadOrder.addShort")}
						</Button>
					</BoxListRow>
				))}
			</BoxList>
		</div>
	);
}

/// The name of a row, with the number the game gets when the row has one, and "Off" when the profile turns the mod off.
function RowLabel({ mod, number }: { mod: OrderRowMod; number?: number }) {
	const { t } = useTranslation();

	return (
		<ItemContent className="min-w-0 flex-row items-center gap-3">
			{number !== undefined && <span className="text-muted-foreground w-10 shrink-0 font-mono text-xs tabular-nums">{number}</span>}
			<ItemTitle className="min-w-0 flex-1 truncate font-normal">{mod.name}</ItemTitle>
			{!mod.enabled && <span className="text-muted-foreground shrink-0 text-xs">{t("loadOrder.off")}</span>}
		</ItemContent>
	);
}

/// React moves a row's element when the row changes place and removes it when the row goes, and both drop the keyboard
/// focus. In the next frame, once the rows are drawn again, the focus goes to the first enabled control in `controls` of
/// the row now at `index` (the last row when `index` is past the end), as the mod list does after Alt+arrows.
function restoreFocus(list: RefObject<HTMLElement | null>, index: number, controls: readonly string[]) {
	requestAnimationFrame(() => {
		const rows = list.current?.querySelectorAll("li") ?? [];
		const row = rows[Math.min(index, rows.length - 1)];

		for (const control of controls) {
			const button = row?.querySelector<HTMLButtonElement>(`[data-action="${control}"]`);
			if (button && !button.disabled) {
				button.focus();
				return;
			}
		}
	});
}

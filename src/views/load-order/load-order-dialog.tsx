import { type ReactNode, useState } from "react";

import type { LoadOrderRange } from "@/bindings";
import { SectionHeading } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import {
	availableMods,
	capacity,
	fits,
	moveBy,
	nothingOrderable,
	orderableMods,
	orderFromMods,
	sameOrder,
	unorderedNumbers,
	withAppended,
	withoutId,
} from "@/lib/load-order";
import type { Mod } from "@/lib/mods";
import { AvailableRows, OrderedRows, type OrderRowMod } from "@/views/load-order/load-order-rows";

/// The row the window draws for a mod: its name, and whether the profile turns it on.
function rowOf(mod: Mod): OrderRowMod {
	return { id: mod.id, name: mod.info.name, enabled: mod.enabled };
}

/// The load order of the active profile. The user picks the mods that take part and puts them in order; the game gets
/// one number per mod from the range its definition names. Mods left out keep no number. Only a mod whose folder the
/// game reads in order can take part.
export function LoadOrderDialog({
	mods,
	range,
	settingsReady,
	onSave,
	onClose,
}: {
	mods: Mod[];
	range: LoadOrderRange;
	/// Whether the folder of the game's settings is chosen. Without it the numbers have nowhere to go.
	settingsReady: boolean;
	/// Resolves true once the order is saved. Anything else keeps the window open with the order the user made.
	onSave: (order: string[]) => Promise<boolean>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	// The order the profile had when the window opened, so Save is only offered for a real change.
	const [saved] = useState(() => orderFromMods(orderableMods(mods)));
	// What the user picked. It can name a mod that has left the profile since, so the rows below use `order`.
	const [picked, setPicked] = useState(saved);
	const [busy, setBusy] = useState(false);
	const [failed, setFailed] = useState(false);

	const byId = new Map<string, OrderRowMod>(orderableMods(mods).map((mod) => [mod.id, rowOf(mod)]));
	const order = picked.filter((id) => byId.has(id));
	const ordered = order.flatMap((id) => byId.get(id) ?? []);
	const available = availableMods(mods, order).map((mod) => rowOf(mod));
	const dropped = unorderedNumbers(mods);
	const tooMany = !fits(order, range);
	const changed = !sameOrder(order, saved);

	// The window closes only after a save that worked. While a save runs, Cancel and the close button do nothing, so the
	// picked order is still here if the save fails.
	const close = () => {
		if (!busy) onClose();
	};

	// A save that fails keeps the picked order on screen and shows a message; the failure itself is already a notice.
	const save = async () => {
		setBusy(true);
		setFailed(false);
		const ok = await onSave(order);
		setBusy(false);

		if (ok) onClose();
		else setFailed(true);
	};

	return (
		<ModalDialog title={t("loadOrder.title")} description={t("loadOrder.hint")} width="lg" onClose={close}>
			{!settingsReady && <p className="text-sm">{t("loadOrder.noSettings")}</p>}
			{tooMany && (
				<p role="alert" className="text-destructive text-sm">
					{t("loadOrder.tooMany", { count: capacity(range) })}
				</p>
			)}
			{dropped.length > 0 && (
				<p className="text-muted-foreground text-sm">{t("loadOrder.dropped", { names: dropped.map((mod) => mod.info.name).join(", ") })}</p>
			)}

			{nothingOrderable(mods) ? (
				<NoRows>{t("loadOrder.noneOrderable")}</NoRows>
			) : (
				<>
					<section className="grid gap-2">
						<SectionHeading title={t("loadOrder.ordered")} hint={t("loadOrder.orderedHint")} />
						{ordered.length === 0 ? (
							<NoRows>{t("loadOrder.orderedEmpty")}</NoRows>
						) : (
							<OrderedRows
								mods={ordered}
								range={range}
								busy={busy}
								onMove={(index, step) => setPicked(moveBy(order, index, step))}
								onRemove={(id) => setPicked(withoutId(order, id))}
							/>
						)}
					</section>

					<section className="grid gap-2">
						<SectionHeading title={t("loadOrder.available")} hint={t("loadOrder.availableHint")} />
						{available.length === 0 ? (
							<NoRows>{t("loadOrder.availableEmpty")}</NoRows>
						) : (
							<AvailableRows mods={available} busy={busy} onAdd={(id) => setPicked(withAppended(order, id))} />
						)}
					</section>
				</>
			)}

			{failed && <FieldError>{t("loadOrder.saveFailed")}</FieldError>}

			<DialogFooter>
				<Button variant="ghost" disabled={order.length === 0 || busy} onClick={() => setPicked([])}>
					{t("loadOrder.clear")}
				</Button>
				<Button variant="outline" disabled={busy} onClick={close}>
					{t("common.cancel")}
				</Button>
				<Button disabled={!changed || tooMany || busy} onClick={() => void save()}>
					{busy ? t("loadOrder.saving") : t("loadOrder.save")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

/// The note in place of a list with no rows.
function NoRows({ children }: { children: ReactNode }) {
	return (
		<Empty>
			<EmptyHeader>
				<EmptyTitle>{children}</EmptyTitle>
			</EmptyHeader>
		</Empty>
	);
}

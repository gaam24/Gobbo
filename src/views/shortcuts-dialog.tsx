import { Fragment } from "react";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { useTranslation } from "@/i18n";
import { comboTokens, type ShortcutContext, shortcut, shortcutIds } from "@/lib/shortcuts";

const SECTIONS: readonly ShortcutContext[] = ["general", "navigation", "list"];

const MOUSE_TIPS = ["rightClick", "doubleClick", "drag", "columns", "separator", "drop"] as const;

/// Key names the locale translates; the rest (letters, arrows, F keys) show as they are.
const KEY_LABELS = {
	Ctrl: "bar.keys.ctrl",
	Alt: "bar.keys.alt",
	Shift: "shortcuts.keys.shift",
	Space: "bar.keys.space",
	Esc: "shortcuts.keys.esc",
	Click: "shortcuts.keys.click",
	MouseBack: "shortcuts.keys.mouseBack",
	MouseForward: "shortcuts.keys.mouseForward",
} as const;

const isNamed = (key: string): key is keyof typeof KEY_LABELS => key in KEY_LABELS;

/// Every shortcut of the app on one page, and the mouse moves that are easy to miss.
export function ShortcutsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
	const { t } = useTranslation();
	const keyName = (key: string) => (isNamed(key) ? t(KEY_LABELS[key]) : key);

	return (
		<ModalDialog open={open} width="3xl" title={t("shortcuts.title")} description={t("shortcuts.hint")} onClose={() => onOpenChange(false)}>
			<div className="grid gap-x-10 gap-y-6 md:grid-cols-2">
				{SECTIONS.map((section) => (
					<section key={section} className="grid content-start gap-2">
						<SectionHeading title={t(`shortcuts.sections.${section}`)} />
						<dl className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-2 text-sm">
							{shortcutIds
								.filter((id) => shortcut(id).context === section)
								.map((id) => {
									const { labelKey, keys } = shortcut(id);
									return (
										<Fragment key={id}>
											<dt className="text-muted-foreground">{t(labelKey)}</dt>
											<dd className="grid justify-items-end gap-1">
												{keys.map((combo, index) => {
													const tokens = comboTokens(combo);
													return (
														<span key={tokens.join("+")} className="flex items-center gap-1 whitespace-nowrap">
															{index > 0 && <span className="text-subtle-foreground mr-0.5 text-xs">{t("shortcuts.or")}</span>}
															<KbdGroup>
																{tokens.map((key) => (
																	<Kbd key={key}>{keyName(key)}</Kbd>
																))}
															</KbdGroup>
														</span>
													);
												})}
											</dd>
										</Fragment>
									);
								})}
						</dl>
					</section>
				))}
			</div>
			<section className="grid gap-2 border-t pt-4">
				<SectionHeading title={t("shortcuts.sections.mouse")} />
				<BoxList>
					{MOUSE_TIPS.map((tip) => (
						<BoxListRow key={tip}>
							<ItemContent>
								<ItemTitle className="w-full font-normal whitespace-normal">{t(`shortcuts.mouse.${tip}`)}</ItemTitle>
							</ItemContent>
						</BoxListRow>
					))}
				</BoxList>
			</section>
			<DialogCloseFooter onClose={() => onOpenChange(false)} />
		</ModalDialog>
	);
}

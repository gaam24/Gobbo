import { useDeferredValue, useMemo, useRef, useState } from "react";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useTranslation } from "@/i18n";
import { PALETTE_GROUPS, type PaletteItem, paletteResults } from "@/lib/palette";

/// Mounted while open, so each opening starts empty. Everything in one place: type a few letters of an action, a mod, a profile or a game, and press Enter. The arrows
/// move through the results; the palette closes before the item runs, so a dialog it opens gets the focus.
export function CommandPalette({
	open,
	onOpenChange,
	items,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	items: PaletteItem[];
}) {
	const { t } = useTranslation();
	const [query, setQuery] = useState("");
	/// The element that opened the palette. Radix restores the focus to whatever was focused when the dialog opened; a
	/// click on a button does not always focus it, so the opener is remembered here and focused on close.
	const opener = useRef<HTMLElement | null>(document.activeElement instanceof HTMLElement ? document.activeElement : null);
	/// The list may hold every mod of a large profile. The field stays responsive while the results catch up.
	const deferredQuery = useDeferredValue(query);
	const results = useMemo(() => paletteResults(items, deferredQuery), [items, deferredQuery]);

	const run = (item: PaletteItem) => {
		if (item.disabled) return;
		onOpenChange(false);
		// After the dialog has let go of the focus.
		requestAnimationFrame(() => void item.run());
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				showCloseButton={false}
				aria-describedby={undefined}
				onCloseAutoFocus={(event) => {
					event.preventDefault();
					opener.current?.focus();
				}}
				className="top-[12vh] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
			>
				<DialogTitle className="sr-only">{t("palette.title")}</DialogTitle>
				<Command shouldFilter={false} className="rounded-none bg-transparent p-0">
					<CommandInput
						autoFocus
						aria-label={t("palette.title")}
						placeholder={t("palette.placeholder")}
						value={query}
						onValueChange={setQuery}
					/>
					<CommandList className="max-h-[60vh]">
						<CommandEmpty className="px-3 py-6 text-center text-sm">{t("palette.empty")}</CommandEmpty>
						{PALETTE_GROUPS.map((group) => {
							const inGroup = results.filter((item) => item.group === group);
							if (inGroup.length === 0) return null;
							return (
								<CommandGroup key={group} heading={t(`palette.groups.${group}`)}>
									{inGroup.map((item) => {
										const Icon = item.icon;
										return (
											<CommandItem key={item.id} value={item.id} disabled={item.disabled} onSelect={() => run(item)}>
												{Icon ? (
													<Icon className="text-muted-foreground size-4 shrink-0" aria-hidden />
												) : (
													<span className="size-4 shrink-0" />
												)}
												<span className="min-w-0 flex-1 truncate">{item.label}</span>
												{item.disabled && item.reason ? (
													<span className="text-muted-foreground shrink-0 text-xs">{item.reason}</span>
												) : (
													item.hint && <CommandShortcut>{item.hint}</CommandShortcut>
												)}
											</CommandItem>
										);
									})}
								</CommandGroup>
							);
						})}
					</CommandList>
					<p className="text-subtle-foreground flex gap-4 border-t px-4 py-2 text-xs">
						<span>{t("palette.footer.move")}</span>
						<span>{t("palette.footer.run")}</span>
						<span>{t("palette.footer.close")}</span>
					</p>
				</Command>
			</DialogContent>
		</Dialog>
	);
}

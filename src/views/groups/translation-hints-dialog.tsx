import { useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { ModGroup } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { ItemActions, ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { suggestTranslationPairs, type TranslationHint } from "@/lib/translation-hints";
import { isTextList, readPref, writePref } from "@/lib/ui-prefs";
import { nextGroupId } from "@/views/groups/group-edit-dialog";

const DISMISSED_PREF = "translationHints.dismissed";

const pairKey = (hint: TranslationHint) => `${hint.baseId}|${hint.translationId}`;

/// Suggestions the user still has to confirm. Nothing here changes the list until Accept, and a dismissed pair is
/// remembered in a local pref so the same hint is not offered again.
export function TranslationHintsDialog({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	const { status, gameActions } = useWorkspace();
	const [dismissed, setDismissed] = useState<string[]>(() => readPref<string[]>(DISMISSED_PREF, [], isTextList));
	const groups = status?.groups ?? [];
	const hints = suggestTranslationPairs(status?.mods ?? [], groups).filter((hint) => !dismissed.includes(pairKey(hint)));
	const nameOf = (id: string) => status?.mods.find((mod) => mod.id === id)?.info.name ?? id;

	const dismiss = (hint: TranslationHint) => {
		const key = pairKey(hint);
		if (dismissed.includes(key)) return;
		const next = [...dismissed, key];
		setDismissed(next);
		writePref(DISMISSED_PREF, next);
	};

	const accept = (hint: TranslationHint) => {
		const language = hint.language ?? t("mods.hints.unknownLanguage");
		const group: ModGroup = {
			id: nextGroupId(groups),
			name: t("mods.hints.groupName", { name: nameOf(hint.baseId), language }),
			members: [hint.baseId, hint.translationId],
			kind: "translation",
			language: hint.language,
		};
		onClose();
		void gameActions.saveModGroups([...groups, group]);
	};

	return (
		<ModalDialog title={t("mods.hints.title")} description={t("mods.hints.hint")} width="xl" onClose={onClose}>
			{hints.length === 0 ? (
				<p className="text-muted-foreground text-sm">{t("mods.hints.empty")}</p>
			) : (
				<BoxList scroll tall>
					{hints.map((hint) => {
						const language = hint.language ?? t("mods.hints.unknownLanguage");
						const baseName = nameOf(hint.baseId);
						const translationName = nameOf(hint.translationId);
						const groupName = t("mods.hints.groupName", { name: baseName, language });
						return (
							<BoxListRow key={pairKey(hint)}>
								<ItemContent className="w-full min-w-0 gap-2.5">
									<div className="min-w-0">
										{/* The pair fills its own line; buttons never share this row with the names. */}
										<ItemTitle className="line-clamp-2 w-full min-w-0 font-medium text-pretty break-words">
											{t("mods.hints.pair", { base: baseName, translation: translationName })}
										</ItemTitle>
										<p className="text-muted-foreground mt-1 text-xs">
											{t("mods.hints.meta", { language, percent: Math.round(hint.confidence * 100) })}
										</p>
									</div>

									{/* A compact preview of the group this pair would become. */}
									<div className="border-border bg-muted/30 rounded-md border p-2.5">
										<p className="text-muted-foreground mb-1.5 text-xs">{t("mods.hints.previewTitle")}</p>
										<p className="min-w-0 text-xs font-medium break-words">{groupName}</p>
										<div className="mt-1.5 flex flex-col gap-1 text-xs">
											<div className="bg-card flex min-w-0 items-center gap-2 rounded-sm px-2 py-1">
												<span className="text-muted-foreground shrink-0">{t("mods.hints.baseLabel")}</span>
												<span className="min-w-0 truncate">{baseName}</span>
												<span className="text-subtle-foreground ml-auto shrink-0">{t("mods.hints.headerLabel")}</span>
											</div>
											<div className="border-border flex min-w-0 items-center gap-2 border-l-2 py-1 pl-3">
												<span className="text-muted-foreground shrink-0">{t("mods.hints.translationLabel")}</span>
												<span className="min-w-0 truncate">{translationName}</span>
											</div>
										</div>
										<p className="text-muted-foreground mt-1.5 text-xs text-pretty">{t("mods.hints.previewNote")}</p>
									</div>

									<ItemActions className="border-border justify-end border-t pt-2.5">
										<Button size="sm" variant="ghost" onClick={() => dismiss(hint)}>
											{t("mods.hints.dismiss")}
										</Button>
										<Button size="sm" onClick={() => accept(hint)}>
											{t("mods.hints.accept")}
										</Button>
									</ItemActions>
								</ItemContent>
							</BoxListRow>
						);
					})}
				</BoxList>
			)}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.close")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

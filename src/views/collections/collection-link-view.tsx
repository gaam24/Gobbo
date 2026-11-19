import { Boxes } from "lucide-react";
import { useState } from "react";

import { useCollections } from "@/app/contexts/collections-context";
import { useSettingsData } from "@/app/contexts/settings-context";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";

/// The first screen of the collection dialog: a collection or modpack link.
export function CollectionLinkView({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	const { busy, failure, openPreview } = useCollections();
	const { providers } = useSettingsData();
	const [link, setLink] = useState("");
	const sources = providers
		.filter((provider) => provider.collections)
		.map((provider) => provider.name)
		.join(", ");

	const submit = () => {
		if (!link.trim() || busy) return;
		void openPreview(link.trim());
	};

	return (
		<ModalDialog title={t("collection.dialog.link")} description={t("collection.link.description", { sources })} onClose={onClose}>
			<Field>
				<FieldLabel htmlFor="collection-link">{t("collection.link.label")}</FieldLabel>
				<Input
					id="collection-link"
					value={link}
					placeholder={t("collection.link.placeholder")}
					spellCheck={false}
					autoFocus
					onChange={(event) => setLink(event.currentTarget.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter") submit();
					}}
				/>
				<FieldDescription className="whitespace-pre-line">{t("collection.link.hint")}</FieldDescription>
			</Field>
			{failure && (
				<p role="alert" className="text-destructive text-sm">
					{failure}
				</p>
			)}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={busy || !link.trim()} onClick={submit}>
					<Boxes /> {busy ? t("collection.link.reading") : t("collection.link.open")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

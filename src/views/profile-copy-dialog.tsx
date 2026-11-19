import { useState } from "react";
import type { ProfileView } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";

/// Chooses the profile whose list and data replace the active profile's. The backend keeps the earlier data as a
/// snapshot, so the copy can be undone.
export function ProfileCopyDialog({
	profiles,
	target,
	open,
	onClose,
	onCopy,
}: {
	profiles: ProfileView[];
	target: ProfileView;
	open: boolean;
	onClose: () => void;
	onCopy: (source: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const others = profiles.filter((profile) => profile.id !== target.id);
	const [chosen, setChosen] = useState<string | null>(null);
	const source = others.find((profile) => profile.id === chosen) ?? others[0] ?? null;

	return (
		<ModalDialog
			title={t("profiles.copy.title", { target: target.name })}
			description={t("profiles.copy.hint")}
			open={open}
			onClose={onClose}
		>
			{others.length > 0 && (
				<Field>
					<FieldLabel htmlFor="copy-profile">{t("profiles.copy.source")}</FieldLabel>
					<Select value={source?.id ?? undefined} onValueChange={setChosen}>
						<SelectTrigger id="copy-profile" className="w-full">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{others.map((profile) => (
								<SelectItem key={profile.id} value={profile.id}>
									{profile.name}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</Field>
			)}
			<p className="text-muted-foreground text-sm">{t("profiles.copy.body", { target: target.name })}</p>
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button
					disabled={!source}
					onClick={() => {
						if (!source) return;
						void onCopy(source.id);
						onClose();
					}}
				>
					{t("profiles.copy.action")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

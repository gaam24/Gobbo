import { useState } from "react";
import { commands, type ZipInspect } from "@/bindings";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import type { FomodChoice } from "@/lib/fomod";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";
import { FomodWizard } from "@/views/fomod/fomod-wizard";

/// Reinstalls a FOMOD mod with new installer options. The wizard starts from the answers the version was installed
/// with; the archive copy is read again, so a changed installer definition is respected.
export function ReinstallDialog({
	gameId,
	folder,
	name,
	onConfirm,
	onClose,
}: {
	gameId: string;
	folder: string;
	name: string;
	onConfirm: (choices: FomodChoice[]) => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	const inspect = useLoad<ZipInspect>(`inspect\n${gameId}\n${folder}`, () => unwrap(commands.reinstallInspect(gameId, folder)), {
		onError: (error) => setFailure(messageOf(error)),
	});
	const details = useLoad(`${gameId}\n${folder}`, () => unwrap(commands.modDetails(gameId, folder)), {
		onError: (error) => setFailure(messageOf(error)),
	});

	if (inspect?.kind === "fomod" && details) {
		return (
			<FomodWizard
				moduleName={inspect.module_name}
				steps={inspect.steps}
				fileName={name}
				game={gameId}
				target={{ kind: "reinstall", folder }}
				initialChoices={details.extras.fomod}
				onConfirm={(choices) => void onConfirm(choices)}
				onSkip={onClose}
			/>
		);
	}

	let body = <p className="text-muted-foreground text-sm">{t("fomod.reading")}</p>;
	if (failure) {
		body = <FieldError>{t("fomod.readFailed", { detail: failure })}</FieldError>;
	} else if (inspect?.kind === "plain") {
		body = <p className="text-muted-foreground text-sm">{t("fomod.noInstaller")}</p>;
	}

	return (
		<ModalDialog title={t("mods.changeInstaller")} description={t("fomod.reinstallHint", { name })} width="lg" onClose={onClose}>
			{body}
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

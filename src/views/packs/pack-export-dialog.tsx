import { useState } from "react";
import { commands, type PackArchives, type PackOptions, type ProfileView } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import { messageOf, translate, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

const ARCHIVE_CHOICES = ["none", "unlisted", "all"] as const satisfies readonly PackArchives[];

/// A pack is a recipe: the mods with their exact files, FOMOD choices, order, separators and rules. The receiver gets
/// the files from Nexus; archives travel inside only when chosen. The check below lists what would stop a rebuild.
export function PackExportDialog({
	gameId,
	profiles,
	onExport,
	onClose,
}: {
	gameId: string;
	profiles: ProfileView[];
	onExport: (options: PackOptions) => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const active = profiles.find((profile) => profile.active) ?? profiles[0];
	const [info, setInfo] = useState({ name: active?.name ?? "", version: "1.0", author: "", description: "" });
	const [chosen, setChosen] = useState<string[]>(active ? [active.id] : []);
	const [archives, setArchives] = useState<PackArchives>("unlisted");
	const [failure, setFailure] = useState<{ key: string; message: string } | null>(null);
	const [exporting, setExporting] = useState(false);
	const options: PackOptions = { info: { id: "", ...info }, profiles: chosen, archives };
	// The backend check only looks at the name, the version, the profiles and the archives. Author and description do
	// not change its answer, so editing them does not start another check.
	const checkKey = JSON.stringify({ name: info.name, version: info.version, profiles: chosen, archives });
	const checkFailure = failure?.key === checkKey ? failure.message : null;
	const issues = useLoad(checkKey, () => unwrap(commands.checkPack(gameId, options)), {
		onError: (error) => setFailure({ key: checkKey, message: messageOf(error) }),
	});
	const errors = issues?.filter((issue) => issue.error) ?? [];
	const field = (key: keyof typeof info) => (value: string) => setInfo({ ...info, [key]: value });
	const exportNow = async () => {
		if (exporting) return;
		setExporting(true);
		try {
			await onExport(options);
		} catch {
			// The parent reports the failure; the dialog stays usable for another try.
		}
		setExporting(false);
	};

	return (
		<ModalDialog title={t("pack.exportTitle")} description={t("pack.exportHint")} onClose={onClose}>
			<FieldGroup className="gap-4">
				<div className="grid grid-cols-[minmax(0,2fr)_minmax(0,1fr)] gap-3">
					<Field>
						<FieldLabel htmlFor="pack-name">{t("pack.name")}</FieldLabel>
						<Input id="pack-name" value={info.name} onChange={(event) => field("name")(event.currentTarget.value)} />
					</Field>
					<Field>
						<FieldLabel htmlFor="pack-version">{t("pack.version")}</FieldLabel>
						<Input id="pack-version" value={info.version} onChange={(event) => field("version")(event.currentTarget.value)} />
					</Field>
				</div>
				<Field>
					<FieldLabel htmlFor="pack-author">{t("pack.author")}</FieldLabel>
					<Input id="pack-author" value={info.author} onChange={(event) => field("author")(event.currentTarget.value)} />
				</Field>
				<Field>
					<FieldLabel htmlFor="pack-description">{t("pack.description")}</FieldLabel>
					<Textarea
						id="pack-description"
						rows={3}
						value={info.description}
						onChange={(event) => field("description")(event.currentTarget.value)}
					/>
				</Field>
				<FieldSet className="gap-2">
					<FieldLegend variant="label">{t("pack.profiles")}</FieldLegend>
					{profiles.map((profile) => (
						<Field key={profile.id} orientation="horizontal">
							<Checkbox
								id={`pack-profile-${profile.id}`}
								checked={chosen.includes(profile.id)}
								onCheckedChange={(checked) =>
									setChosen(checked === true ? [...chosen, profile.id] : chosen.filter((id) => id !== profile.id))
								}
							/>
							<FieldLabel htmlFor={`pack-profile-${profile.id}`}>{profile.name}</FieldLabel>
							<span className="text-subtle-foreground text-xs tabular-nums">{t("pack.profileCount", { count: profile.mod_count })}</span>
						</Field>
					))}
				</FieldSet>
				<Field>
					<FieldLabel htmlFor="pack-archives">{t("pack.archives")}</FieldLabel>
					<Select value={archives} onValueChange={(value) => setArchives(ARCHIVE_CHOICES.find((choice) => choice === value) ?? "unlisted")}>
						<SelectTrigger id="pack-archives">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{ARCHIVE_CHOICES.map((choice) => (
								<SelectItem key={choice} value={choice}>
									{t(`pack.archivesChoice.${choice}`)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					<FieldDescription>{t(`pack.archivesHint.${archives}`)}</FieldDescription>
				</Field>
				{checkFailure && <FieldError>{checkFailure}</FieldError>}
				{issues && issues.length > 0 && (
					<Alert variant={errors.length > 0 ? "destructive" : "default"}>
						<AlertTitle>{t(errors.length > 0 ? "pack.check.errors" : "pack.check.warnings")}</AlertTitle>
						<AlertDescription>
							<ul className="list-disc pl-4">
								{issues.map((issue) => (
									<li key={`${issue.message.code}${JSON.stringify(issue.message.params)}`}>{translate(issue.message)}</li>
								))}
							</ul>
						</AlertDescription>
					</Alert>
				)}
				{issues?.length === 0 && <p className="text-muted-foreground text-sm">{t("pack.check.clean")}</p>}
			</FieldGroup>
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button
					disabled={errors.length > 0 || chosen.length === 0 || issues === undefined || checkFailure !== null || exporting}
					aria-busy={exporting}
					onClick={() => void exportNow()}
				>
					{t("pack.save")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

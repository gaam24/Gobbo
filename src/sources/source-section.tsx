import { CircleArrowUp, ExternalLink, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { AppError, UpdateReport } from "@/bindings";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { CommandError, errorText, messageOf } from "@/lib/ipc";
import { type FullMod, formatDate, formatTime, isToday } from "@/lib/mods";
import { useSourceName } from "@/sources/source-name";
import { UpdateLine } from "@/sources/update-line";

interface SourceSectionProps {
	mod: FullMod;
	update?: UpdateReport;
	checkedAt?: number;
	onSetSource: (modId: string, provider: string, sourceId: string | null) => void | Promise<void>;
	onCheck: (folder: string) => Promise<UpdateReport>;
	onIgnore: (modId: string, provider: string) => void | Promise<void>;
	onUnignore: (modId: string, provider: string) => void | Promise<void>;
	onOpenGames: () => void;
	onError: (message: string) => void;
}

interface CheckFailure {
	text: string;
	kind: AppError["kind"];
}

/// The provider pages of a mod: the ids the user typed, the page links, the update check and its result. A provider a
/// game defines can be added here; the backend owns the id validation.
export function SourceSection({
	mod,
	update,
	checkedAt,
	onSetSource,
	onCheck,
	onIgnore,
	onUnignore,
	onOpenGames,
	onError,
}: SourceSectionProps) {
	const { t, i18n } = useTranslation();
	const { game, openSource } = useWorkspace();
	const sourceName = useSourceName();
	const [checking, setChecking] = useState(false);
	const [failure, setFailure] = useState<CheckFailure | null>(null);
	const [provider, setProvider] = useState("");
	const [newId, setNewId] = useState("");
	const available = (game?.providers ?? []).filter((item) => !mod.sources.some((source) => source.provider === item.id));
	const chosen = provider !== "" ? provider : (available[0]?.id ?? "");
	const canAdd = chosen !== "" && newId.trim() !== "";
	const shownFailure = failure ?? (update?.error ? { text: errorText(update.error), kind: update.error.kind } : null);
	const { providers, readyProviders } = useSettingsData();
	/// The first provider of this mod whose key is needed and not saved; its check would only fail.
	const missingKey = providers.find(
		(item) => item.needs_key && !readyProviders.has(item.id) && mod.sources.some((source) => source.provider === item.id),
	);

	const check = async () => {
		setChecking(true);
		setFailure(null);

		try {
			const report = await onCheck(mod.id);

			if (report.error?.kind === "bug") onError(errorText(report.error));
		} catch (error) {
			const kind = error instanceof CommandError ? error.appError.kind : "retryable";
			setFailure({ text: messageOf(error), kind });
		}
		// Kept outside try/finally: a finalizer makes React Compiler skip this whole component.
		setChecking(false);
	};

	const checkedText = () => {
		if (checkedAt === undefined) return t("mods.sources.neverChecked");

		if (isToday(checkedAt))
			return t("mods.sources.checkedToday", {
				time: formatTime(checkedAt, i18n.language),
			});
		return t("mods.sources.checkedOn", {
			date: formatDate(checkedAt, i18n.language),
		});
	};
	const openUpdate =
		update?.has_update && update.latest && !update.latest_installed ? () => openSource(update.provider, update.mod_id, "files") : undefined;
	const ignoreUpdate = update?.latest && update.has_update ? () => void onIgnore(mod.id, update.provider) : undefined;
	const showUpdate = update?.state === "ignored" ? () => void onUnignore(mod.id, update.provider) : undefined;

	return (
		<div className="grid gap-4">
			<div className="grid gap-2">
				<span className="text-sm font-semibold">{t("mods.sources.title")}</span>
				<p className="text-muted-foreground text-xs">{t("mods.sources.hint")}</p>
				{mod.sources.length === 0 ? (
					<p className="border-border text-subtle-foreground rounded-lg border border-dashed px-3 py-3 text-xs">
						{t("mods.sources.empty")}
					</p>
				) : (
					<ul className="grid gap-2">
						{mod.sources.map((source) => (
							<SourceRow
								key={`${source.provider}:${source.mod_id}`}
								mod={mod}
								provider={source.provider}
								modId={source.mod_id}
								pageUrl={source.page_url ?? null}
								onSave={onSetSource}
								onOpen={() => openSource(source.provider, source.mod_id)}
							/>
						))}
					</ul>
				)}
			</div>

			{available.length > 0 && (
				<form
					className="border-border bg-muted/25 grid gap-2 rounded-lg border p-3"
					onSubmit={(event) => {
						event.preventDefault();

						if (!canAdd) return;
						void onSetSource(mod.id, chosen, newId.trim());
						setNewId("");
					}}
				>
					<span className="text-subtle-foreground text-xs">{t("mods.sources.addHint")}</span>
					<div className="grid grid-cols-[8rem_minmax(0,1fr)_auto] gap-2">
						<Select value={chosen} onValueChange={setProvider}>
							<SelectTrigger className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{available.map((item) => (
									<SelectItem key={item.id} value={item.id}>
										{sourceName(item.id)}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<Input
							aria-label={t("mods.sources.id")}
							placeholder={t("mods.sources.idPlaceholder")}
							value={newId}
							onChange={(event) => setNewId(event.currentTarget.value)}
						/>
						<Button type="submit" variant="outline" size="sm" disabled={!canAdd}>
							<Plus /> {t("mods.sources.add")}
						</Button>
					</div>
				</form>
			)}

			<div className="grid gap-2 text-sm">
				{!shownFailure && <UpdateLine update={update} onOpen={openUpdate} onIgnore={ignoreUpdate} onUnignore={showUpdate} />}
				<p className="text-muted-foreground text-xs">{checkedText()}</p>
			</div>

			{missingKey && (
				<Alert>
					<AlertDescription className="grid gap-2">
						<span>{t("mods.sources.needKey", { provider: missingKey.name })}</span>
						<Button variant="outline" size="sm" className="justify-self-start" onClick={onOpenGames}>
							{t("mods.sources.goToGames")}
						</Button>
					</AlertDescription>
				</Alert>
			)}

			{shownFailure && (
				<Alert variant="destructive">
					<AlertDescription className="grid gap-2">
						<span>{shownFailure.text}</span>
						{shownFailure.kind === "user_action" && (
							<Button variant="outline" size="sm" className="justify-self-start" onClick={onOpenGames}>
								{t("mods.sources.goToGames")}
							</Button>
						)}
						{shownFailure.kind !== "user_action" && shownFailure.kind !== "permanent" && (
							<Button variant="outline" size="sm" className="justify-self-start" disabled={checking} onClick={() => void check()}>
								{t("common.retry")}
							</Button>
						)}
					</AlertDescription>
				</Alert>
			)}

			<Button variant="outline" size="sm" className="justify-self-start" disabled={checking} onClick={() => void check()}>
				{checking ? <Spinner aria-hidden /> : <CircleArrowUp />} {checking ? t("mods.sources.checking") : t("mods.sources.check")}
			</Button>
		</div>
	);
}

/// One source: its provider and id, a page link, an editable id and a remove button.
function SourceRow({
	mod,
	provider,
	modId,
	pageUrl,
	onSave,
	onOpen,
}: {
	mod: FullMod;
	provider: string;
	modId: string;
	pageUrl: string | null;
	onSave: (modId: string, provider: string, sourceId: string | null) => void | Promise<void>;
	onOpen: () => void;
}) {
	const { t } = useTranslation();
	const sourceName = useSourceName();
	const [value, setValue] = useState(modId);
	const [error, setError] = useState<string | null>(null);
	const changed = value.trim() !== modId;

	const save = async () => {
		if (value.trim() === "") {
			setError(t("mods.sources.idEmpty"));
			return;
		}

		setError(null);
		await onSave(mod.id, provider, value.trim());
	};

	return (
		<li className="border-border bg-muted/25 grid gap-2 rounded-lg border p-3">
			<div className="flex min-w-0 items-center justify-between gap-2">
				<div className="flex min-w-0 items-center gap-2">
					<span className="text-sm font-medium">{sourceName(provider)}</span>
					<span className="text-subtle-foreground font-mono text-xs">{`#${modId}`}</span>
				</div>
				<div className="flex gap-1">
					{pageUrl && (
						<Button variant="ghost" size="sm" onClick={onOpen}>
							<ExternalLink /> {t("mods.sources.open")}
						</Button>
					)}
					<Button
						variant="ghost"
						size="icon-sm"
						aria-label={t("mods.sources.remove")}
						title={t("mods.sources.remove")}
						onClick={() => void onSave(mod.id, provider, null)}
					>
						<Trash2 />
					</Button>
				</div>
			</div>
			<Field data-invalid={error !== null}>
				<FieldLabel htmlFor={`source-${provider}-${mod.folder}`} className="sr-only">
					{t("mods.sources.id")}
				</FieldLabel>
				<div className="flex gap-2">
					<Input
						id={`source-${provider}-${mod.folder}`}
						value={value}
						aria-invalid={error !== null}
						onChange={(event) => {
							setValue(event.currentTarget.value);
							setError(null);
						}}
					/>
					<Button variant="outline" size="sm" disabled={!changed} onClick={() => void save()}>
						{t("common.save")}
					</Button>
				</div>
				{error && <FieldError>{error}</FieldError>}
				{!pageUrl && <FieldDescription>{t("mods.sources.noPage")}</FieldDescription>}
			</Field>
		</li>
	);
}

import { Check, FolderOpen } from "lucide-react";
import type { EngineSuggestion_Serialize } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { idForName } from "@/views/games/editor/draft";
import { EditorField } from "@/views/games/editor/fields";
import { QuickSection } from "@/views/games/editor/quick-section";
import type { QuickProps } from "@/views/games/editor/quick-types";

/// The folder and the identity: the folder is read once, and its name and id are only suggestions. A field the user
/// already filled is never overwritten.
export function QuickGame({
	draft,
	onChange,
	folder,
	onFolderChange,
	onBrowse,
	inspection,
	inspecting,
	inspectError,
	editing,
	idWarning,
}: QuickProps) {
	const { t } = useTranslation();
	const detectedSteam = inspection?.steam;
	const stores = inspection?.stores;
	const engine = inspection?.engine;

	const setName = (name: string) =>
		onChange({
			...draft,
			name,
			id: editing ? draft.id : idForName(draft, name),
		});

	return (
		<QuickSection number={1} title={t("editor.quick.gameTitle")} hint={t("editor.quick.gameHint")}>
			<EditorField id="editor-folder" label={t("editor.quick.folder")} hint={t("editor.quick.folderHint")} className="gap-2">
				<div className="flex flex-wrap gap-2">
					<Input
						id="editor-folder"
						className="min-w-0 flex-1 font-mono text-xs"
						value={folder}
						placeholder={t("editor.quick.folderPlaceholder")}
						spellCheck={false}
						onChange={(event) => onFolderChange(event.currentTarget.value)}
					/>
					<Button type="button" variant="outline" onClick={onBrowse}>
						<FolderOpen /> {t("editor.quick.browse")}
					</Button>
				</div>
			</EditorField>
			{inspecting && (
				<p className="text-muted-foreground flex items-center gap-2 text-sm" role="status">
					<Spinner aria-hidden /> {t("editor.quick.inspecting")}
				</p>
			)}
			{inspectError && <p className="text-destructive text-sm">{inspectError}</p>}
			{inspection && !inspecting && (
				<div className="grid gap-2 rounded-lg border p-3 text-sm">
					<p className="flex flex-wrap items-center gap-2">
						<Check aria-hidden className="text-primary size-4" />
						<span>{t("editor.quick.inspected", { name: inspection.name })}</span>
						{detectedSteam && (
							<Badge variant="outline">
								{"Steam · "}
								{detectedSteam.app_id}
							</Badge>
						)}
						{stores?.gog.length ? (
							<Badge variant="outline">
								{t("stores.gog")}
								{" · "}
								{stores.gog.join(", ")}
							</Badge>
						) : null}
						{stores?.epic.length ? (
							<Badge variant="outline">
								{t("stores.epic")}
								{" · "}
								{stores.epic.join(", ")}
							</Badge>
						) : null}
						{stores?.lutris.length ? (
							<Badge variant="outline">
								{t("stores.lutris")}
								{" · "}
								{stores.lutris.join(", ")}
							</Badge>
						) : null}
						{inspection.executables.length > 0 && (
							<Badge variant="outline">
								{t("editor.quick.programs", {
									count: inspection.executables.length,
								})}
							</Badge>
						)}
						{engine && <Badge variant="outline">{engineBadgeText(engine)}</Badge>}
					</p>
					{inspection.loaders.length > 0 && (
						<p className="text-muted-foreground">
							{t("editor.quick.loaderFound", {
								loaders: inspection.loaders.map((loader) => `${loader.name}${loader.variant ? ` (${loader.variant})` : ""}`).join(", "),
							})}
						</p>
					)}
					{engine && (
						<p className="text-muted-foreground">
							{t("editor.quick.engineLoader", {
								loader: engine.variant ? `${engine.loader_name} ${engine.variant}` : engine.loader_name,
							})}
						</p>
					)}
					{engine?.root && <p className="text-muted-foreground">{t("editor.quick.engineRoot", { root: engine.root })}</p>}
				</div>
			)}
			<div className="grid gap-4 md:grid-cols-2">
				<EditorField id="editor-name" label={t("editor.quick.name")}>
					<Input id="editor-name" value={draft.name} placeholder="Example Game" onChange={(event) => setName(event.currentTarget.value)} />
				</EditorField>
				<EditorField id="editor-id" label={t("editor.quick.id")} hint={editing ? t("editor.quick.idStable") : t("editor.quick.idHint")}>
					<Input
						id="editor-id"
						className="font-mono text-xs"
						value={draft.id}
						disabled={editing}
						placeholder="example-game"
						spellCheck={false}
						onChange={(event) => onChange({ ...draft, id: event.currentTarget.value })}
					/>
				</EditorField>
			</div>
			{idWarning && <p className="text-destructive text-sm">{idWarning}</p>}
		</QuickSection>
	);
}

/// The engine as its badge names it: the engine's name and, when the preset names the build, its label, such as
/// `Unity (IL2CPP)`. Names are not translated.
function engineBadgeText(engine: EngineSuggestion_Serialize): string {
	return engine.label ? `${engine.engine_name} (${engine.label})` : engine.engine_name;
}

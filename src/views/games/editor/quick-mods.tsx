import { Check, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { loaderId, loaderVariant, placeSummary, setLoader } from "@/views/games/editor/draft";
import { EditorField, SwitchField } from "@/views/games/editor/fields";
import { QuickSection } from "@/views/games/editor/quick-section";
import type { QuickProps } from "@/views/games/editor/quick-types";

/// How the game receives mods. The three cards are starting points: a loader preset, one folder with a folder per mod,
/// or game folders that keep archive paths. Everything rarer is in the advanced form.
export function QuickMods({ draft, onChange, inspection, presets, layout, onLayoutChange, check }: QuickProps) {
	const { t } = useTranslation();
	const usablePresets = presets.filter((preset) => preset.problems.length === 0);
	const brokenPresets = presets.filter((preset) => preset.problems.length > 0);
	const selectedPreset = usablePresets.find((preset) => preset.id === loaderId(draft));
	const detected = inspection?.loaders.find((loader) => loader.id === selectedPreset?.id);
	const places = check?.summary?.game.places ?? placeSummary(draft);
	const layoutPlaces = draft.deploy.full_root ? t("editor.quick.fullRoot") : places.join(", ") || "—";

	return (
		<QuickSection number={2} title={t("editor.quick.modsTitle")} hint={t("editor.quick.modsHint")}>
			{brokenPresets.length > 0 && (
				<p className="text-destructive text-sm">
					{t("editor.quick.presetProblems", { presets: brokenPresets.map((preset) => preset.id).join(", ") })}
				</p>
			)}
			<div className="grid gap-2 sm:grid-cols-3">
				<LayoutChoice
					active={layout === "loader"}
					title={t("editor.quick.layoutLoader")}
					hint={t("editor.quick.layoutLoaderHint")}
					onClick={() => onLayoutChange("loader", usablePresets[0]?.id)}
				/>
				<LayoutChoice
					active={layout === "slot"}
					title={t("editor.quick.layoutSlot")}
					hint={t("editor.quick.layoutSlotHint")}
					onClick={() => onLayoutChange("slot")}
				/>
				<LayoutChoice
					active={layout === "mirror"}
					title={t("editor.quick.layoutMirror")}
					hint={t("editor.quick.layoutMirrorHint")}
					onClick={() => onLayoutChange("mirror")}
				/>
			</div>

			{layout === "loader" && (
				<div className="grid gap-4 md:grid-cols-2">
					<EditorField id="editor-loader" label={t("editor.quick.loader")} hint={t("editor.quick.loaderHint")}>
						<Select
							value={selectedPreset?.id ?? ""}
							onValueChange={(id) => {
								const preset = usablePresets.find((entry) => entry.id === id);
								onChange(setLoader(draft, id, preset?.default_variant ?? ""));
							}}
						>
							<SelectTrigger id="editor-loader" className="w-full">
								<SelectValue placeholder={t("editor.quick.chooseLoader")} />
							</SelectTrigger>
							<SelectContent>
								{usablePresets.map((preset) => (
									<SelectItem key={preset.id} value={preset.id}>
										{preset.name}
										{preset.origin === "user" ? ` · ${t("games.userDefinition")}` : ""}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</EditorField>
					{selectedPreset && (
						<EditorField id="editor-loader-variant" label={t("editor.quick.variant")} hint={t("editor.quick.variantHint")}>
							<Select
								value={loaderVariant(draft) || "default"}
								onValueChange={(variant) => onChange(setLoader(draft, selectedPreset.id, variant === "default" ? "" : variant))}
							>
								<SelectTrigger id="editor-loader-variant" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="default">
										{t("editor.quick.variantDefault", { variant: selectedPreset.default_variant ?? "—" })}
									</SelectItem>
									{selectedPreset.variants.map((variant) => (
										<SelectItem key={variant.name} value={variant.name}>
											{variant.name}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</EditorField>
					)}
					{selectedPreset && (
						<div className="grid gap-2 text-sm md:col-span-2">
							{detected && (
								<p className="text-primary flex items-center gap-2">
									<Check aria-hidden className="size-4" />
									{t("editor.quick.loaderDetected")}
								</p>
							)}
							<p className="text-muted-foreground">{t("editor.quick.loaderPlaces", { places: selectedPreset.places.join(", ") || "—" })}</p>
							{selectedPreset.requires.length > 0 && (
								<p className="text-muted-foreground font-mono text-xs">
									{t("editor.quick.loaderRequires", { files: selectedPreset.requires.join(", ") })}
								</p>
							)}
						</div>
					)}
				</div>
			)}

			{layout === "slot" && (
				<div className="grid gap-4 md:grid-cols-2">
					<EditorField
						id="editor-slot-dir"
						label={t("editor.quick.slotDir")}
						hint={t("editor.quick.slotDirHint")}
						className="md:col-span-2"
					>
						<Input
							id="editor-slot-dir"
							className="font-mono text-xs"
							list="editor-directories"
							value={draft.deploy.slots?.[0]?.dir ?? ""}
							placeholder="Mods"
							spellCheck={false}
							onChange={(event) => {
								const slots = [...(draft.deploy.slots ?? [])];
								slots[0] = { ...(slots[0] ?? { dir: "" }), dir: event.currentTarget.value };
								onChange({ ...draft, deploy: { ...draft.deploy, slots } });
							}}
						/>
					</EditorField>
					<EditorField id="editor-slot-prefix" label={t("editor.quick.slotPrefix")} hint={t("editor.quick.slotPrefixHint")}>
						<Input
							id="editor-slot-prefix"
							value={draft.deploy.slots?.[0]?.prefix ?? ""}
							placeholder="mod"
							onChange={(event) => {
								const slots = [...(draft.deploy.slots ?? [])];
								slots[0] = { ...(slots[0] ?? { dir: "" }), prefix: event.currentTarget.value };
								onChange({ ...draft, deploy: { ...draft.deploy, slots } });
							}}
						/>
					</EditorField>
					<EditorField id="editor-slot-marker" label={t("editor.quick.slotMarker")} hint={t("editor.deploy.slotMarkerHint")}>
						<Input
							id="editor-slot-marker"
							value={draft.deploy.slots?.[0]?.marker ?? ""}
							placeholder="content"
							onChange={(event) => {
								const slots = [...(draft.deploy.slots ?? [])];
								slots[0] = { ...(slots[0] ?? { dir: "" }), marker: event.currentTarget.value };
								onChange({ ...draft, deploy: { ...draft.deploy, slots } });
							}}
						/>
					</EditorField>
					<div className="md:col-span-2">
						<SwitchField
							id="editor-slot-flat"
							label={t("editor.quick.slotFlat")}
							checked={draft.deploy.slots?.[0]?.flat ?? false}
							onChange={(flat) => {
								const slots = [...(draft.deploy.slots ?? [])];
								slots[0] = { ...(slots[0] ?? { dir: "" }), flat };
								onChange({ ...draft, deploy: { ...draft.deploy, slots } });
							}}
						/>
					</div>
				</div>
			)}

			{layout === "mirror" && (
				<div className="grid gap-3">
					{(draft.deploy.mirror ?? []).map((mirror, index) => (
						<div key={mirror.dir} className="flex flex-wrap items-end gap-2">
							<EditorField id={`editor-mirror-${index}`} label={t("editor.quick.mirrorDir")} className="min-w-0 flex-1">
								<Input
									id={`editor-mirror-${index}`}
									className="font-mono text-xs"
									list="editor-directories"
									value={mirror.dir}
									placeholder="Data"
									spellCheck={false}
									onChange={(event) => {
										const mirrors = [...(draft.deploy.mirror ?? [])];
										mirrors[index] = { ...mirrors[index], dir: event.currentTarget.value };
										onChange({ ...draft, deploy: { ...draft.deploy, mirror: mirrors } });
									}}
								/>
							</EditorField>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() =>
									onChange({
										...draft,
										deploy: { ...draft.deploy, mirror: (draft.deploy.mirror ?? []).filter((_, at) => at !== index) },
									})
								}
							>
								{t("editor.removeRow")}
							</Button>
						</div>
					))}
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="w-fit"
						onClick={() => onChange({ ...draft, deploy: { ...draft.deploy, mirror: [...(draft.deploy.mirror ?? []), { dir: "" }] } })}
					>
						<Search /> {t("editor.quick.addMirror")}
					</Button>
				</div>
			)}

			{layout === "custom" && <p className="text-muted-foreground text-sm">{t("editor.quick.customHint")}</p>}

			<p className="text-muted-foreground text-sm">{t("editor.quick.layoutSummary", { places: layoutPlaces })}</p>
			{inspection && inspection.directories.length > 0 && (
				<datalist id="editor-directories">
					{inspection.directories.map((directory) => (
						<option key={directory} value={directory} label={directory}>
							{directory}
						</option>
					))}
				</datalist>
			)}
		</QuickSection>
	);
}

function LayoutChoice({ active, title, hint, onClick }: { active: boolean; title: string; hint: string; onClick: () => void }) {
	return (
		<Button
			type="button"
			variant={active ? "secondary" : "outline"}
			className="h-auto flex-col items-start gap-1 px-3 py-2.5 text-left whitespace-normal"
			aria-pressed={active}
			onClick={onClick}
		>
			<span className="text-sm font-medium">{title}</span>
			<span className="text-muted-foreground text-xs">{hint}</span>
		</Button>
	);
}

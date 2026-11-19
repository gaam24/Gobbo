import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { GameDraft, LoadOrderDraft, LogSourceDraft, RequirementDraft, VerifiedDraft } from "@/views/games/editor/draft";
import { EditorField, LinesField, RepeatableSection, RepeatRow, SwitchField, TextInput } from "@/views/games/editor/fields";

/// The remaining top-level tables: providers, required files, the order file and the test evidence. They are separate
/// from the mod layout, so a game that needs none of them can leave the whole section empty.
export function AdvancedExtras({ draft, onChange }: { draft: GameDraft; onChange: (draft: GameDraft) => void }) {
	const { t } = useTranslation();
	const providers = Object.entries(draft.providers ?? {});
	const requirements = draft.requirements ?? [];

	const setProviders = (next: Record<string, { key?: string }>) => onChange({ ...draft, providers: next });
	const setProvider = (index: number, id: string, key: string) => {
		const next: Record<string, { key?: string }> = {};
		providers.forEach(([name, entry], at) => {
			next[at === index ? id : name] = { ...entry, key: at === index ? key : entry.key };
		});
		setProviders(next);
	};
	const addProvider = () => {
		let id = "nexus";
		let suffix = 2;
		while (id in (draft.providers ?? {})) {
			id = `provider${suffix}`;
			suffix += 1;
		}
		setProviders({ ...draft.providers, [id]: { key: "" } });
	};

	const setRequirements = (next: RequirementDraft[]) => onChange({ ...draft, requirements: next });
	const setRequirement = (index: number, patch: Partial<RequirementDraft>) =>
		setRequirements(requirements.map((entry, at) => (at === index ? { ...entry, ...patch } : entry)));

	const order = draft.load_order;
	const setOrder = (patch: Partial<LoadOrderDraft>) =>
		onChange({ ...draft, load_order: { format: "mods_settings", file: "", ...order, ...patch } });

	const logs = draft.logs ?? [];
	const setLogs = (next: LogSourceDraft[]) => onChange({ ...draft, logs: next });
	const setLog = (index: number, patch: Partial<LogSourceDraft>) =>
		setLogs(logs.map((log, at) => (at === index ? { ...log, ...patch } : log)));
	const logKinds = [
		["script", t("console.logKind.script")],
		["loader", t("console.logKind.loader")],
		["console", t("console.logKind.console")],
		["crash", t("console.logKind.crash")],
		["mod", t("console.logKind.mod")],
		["game", t("console.logKind.game")],
	] as const;

	const verified = draft.verified;
	const setVerified = (patch: Partial<VerifiedDraft>) => onChange({ ...draft, verified: { date: "", ...verified, ...patch } });

	return (
		<div className="grid gap-8">
			<RepeatableSection
				title={t("editor.extras.providers")}
				hint={t("editor.extras.providersHint")}
				addLabel={t("editor.extras.addProvider")}
				onAdd={addProvider}
			>
				{providers.map(([id, entry], index) => (
					<div key={`${id}-${entry.key ?? ""}`} className="grid gap-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto] md:items-end">
						<EditorField id={`editor-provider-id-${index}`} label={t("editor.extras.provider")}>
							<TextInput
								id={`editor-provider-id-${index}`}
								value={id}
								onChange={(next) => setProvider(index, next, entry.key ?? "")}
								placeholder="nexus"
							/>
						</EditorField>
						<EditorField
							id={`editor-provider-key-${index}`}
							label={t("editor.extras.providerKey")}
							hint={t("editor.extras.providerKeyHint")}
						>
							<TextInput
								id={`editor-provider-key-${index}`}
								value={entry.key ?? ""}
								onChange={(key) => setProvider(index, id, key)}
								placeholder="witcher3"
							/>
						</EditorField>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => {
								const next = { ...draft.providers };
								delete next[id];
								setProviders(next);
							}}
						>
							{t("editor.removeRow")}
						</Button>
					</div>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.extras.requirements")}
				hint={t("editor.extras.requirementsHint")}
				addLabel={t("editor.extras.addRequirement")}
				onAdd={() => setRequirements([...requirements, { name: "", path: "", url: "" }])}
			>
				{requirements.map((requirement, index) => (
					<RepeatRow
						// biome-ignore lint/suspicious/noArrayIndexKey: Form rows have no stable id; the index is their identity.
						key={`requirement-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
						title={requirement.name || requirement.path || t("editor.extras.requirement")}
						onRemove={() => setRequirements(requirements.filter((_, at) => at !== index))}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-requirement-name-${index}`} label={t("editor.extras.requirementName")}>
								<TextInput
									id={`editor-requirement-name-${index}`}
									value={requirement.name}
									onChange={(name) => setRequirement(index, { name })}
									placeholder="BepInEx"
								/>
							</EditorField>
							<EditorField id={`editor-requirement-path-${index}`} label={t("editor.extras.requirementPath")}>
								<TextInput
									id={`editor-requirement-path-${index}`}
									value={requirement.path}
									onChange={(path) => setRequirement(index, { path })}
									placeholder="BepInEx/core/BepInEx.dll"
								/>
							</EditorField>
							<EditorField
								id={`editor-requirement-url-${index}`}
								label={t("editor.extras.requirementUrl")}
								hint={t("editor.extras.requirementUrlHint")}
								className="md:col-span-2"
							>
								<TextInput
									id={`editor-requirement-url-${index}`}
									value={requirement.url ?? ""}
									onChange={(url) => setRequirement(index, { url })}
									placeholder="https://github.com/BepInEx/BepInEx/releases"
								/>
							</EditorField>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>
			<div className="mt-4">
				<LinesField
					id="editor-requires"
					label={t("editor.extras.requires")}
					hint={t("editor.extras.requiresHint")}
					value={draft.requires}
					onChange={(requires) => onChange({ ...draft, requires })}
					rows={2}
					placeholder="BepInEx/core/BepInEx.dll"
				/>
			</div>

			<div className="border-t pt-6">
				<h3 className="mb-1 text-sm font-semibold">{t("editor.extras.loadOrder")}</h3>
				<p className="text-muted-foreground mb-4 text-xs">{t("editor.extras.loadOrderHint")}</p>
				<div className="grid gap-4 md:grid-cols-2">
					<EditorField
						id="editor-load-order-file"
						label={t("editor.extras.loadOrderFile")}
						hint={t("editor.extras.loadOrderFileHint")}
						className="md:col-span-2"
					>
						<TextInput
							id="editor-load-order-file"
							value={order?.file ?? ""}
							onChange={(file) => setOrder({ file })}
							placeholder="{documents}/Example Game/mods.settings"
						/>
					</EditorField>
					<EditorField id="editor-priority-min" label={t("editor.extras.priorityMin")}>
						<TextInput
							id="editor-priority-min"
							value={String(order?.priority_min ?? 1)}
							onChange={(value) => setOrder({ priority_min: Number.parseInt(value, 10) || 1 })}
						/>
					</EditorField>
					<EditorField id="editor-priority-max" label={t("editor.extras.priorityMax")}>
						<TextInput
							id="editor-priority-max"
							value={String(order?.priority_max ?? 9999)}
							onChange={(value) => setOrder({ priority_max: Number.parseInt(value, 10) || 9999 })}
						/>
					</EditorField>
				</div>
			</div>

			<div className="border-t pt-6">
				<RepeatableSection
					title={t("editor.extras.logs")}
					hint={t("editor.extras.logsHint")}
					addLabel={t("editor.extras.addLog")}
					onAdd={() => setLogs([...logs, { kind: "game", path: "", pattern: "*.log", limit: 4, recursive: true }])}
				>
					{logs.map((log, index) => (
						<RepeatRow
							// biome-ignore lint/suspicious/noArrayIndexKey: Form rows have no stable id; the index is their identity.
							key={`log-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
							title={log.path || t("editor.extras.log")}
							onRemove={() => setLogs(logs.filter((_, at) => at !== index))}
						>
							<div className="grid gap-4 md:grid-cols-2">
								<EditorField id={`editor-log-kind-${index}`} label={t("editor.extras.logKind")}>
									<Select value={log.kind} onValueChange={(kind) => setLog(index, { kind })}>
										<SelectTrigger id={`editor-log-kind-${index}`} className="w-full">
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											{logKinds.map(([kind, label]) => (
												<SelectItem key={kind} value={kind}>
													{label}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</EditorField>
								<EditorField id={`editor-log-limit-${index}`} label={t("editor.extras.logLimit")}>
									<TextInput
										id={`editor-log-limit-${index}`}
										value={String(log.limit ?? 4)}
										onChange={(value) => setLog(index, { limit: Number.parseInt(value, 10) || 4 })}
									/>
								</EditorField>
								<EditorField id={`editor-log-path-${index}`} label={t("editor.extras.logPath")} hint={t("editor.extras.logPathHint")}>
									<TextInput
										id={`editor-log-path-${index}`}
										value={log.path}
										onChange={(path) => setLog(index, { path })}
										placeholder="r6/logs"
									/>
								</EditorField>
								<EditorField id={`editor-log-pattern-${index}`} label={t("editor.extras.logPattern")}>
									<TextInput
										id={`editor-log-pattern-${index}`}
										value={log.pattern ?? "*.log"}
										onChange={(pattern) => setLog(index, { pattern })}
										placeholder="*.log"
									/>
								</EditorField>
								<SwitchField
									id={`editor-log-recursive-${index}`}
									label={t("editor.extras.logRecursive")}
									hint={t("editor.extras.logRecursiveHint")}
									checked={log.recursive ?? true}
									onChange={(recursive) => setLog(index, { recursive })}
								/>
							</div>
						</RepeatRow>
					))}
				</RepeatableSection>
			</div>

			<div className="border-t pt-6">
				<h3 className="mb-1 text-sm font-semibold">{t("editor.extras.verified")}</h3>
				<p className="text-muted-foreground mb-4 text-xs">{t("editor.extras.verifiedHint")}</p>
				<div className="grid gap-4 md:grid-cols-2">
					<EditorField id="editor-verified-date" label={t("editor.extras.verifiedDate")} hint={t("editor.extras.verifiedDateHint")}>
						<TextInput
							id="editor-verified-date"
							value={verified?.date ?? ""}
							onChange={(date) => setVerified({ date })}
							placeholder="2026-11-13"
						/>
					</EditorField>
					<LinesField
						id="editor-verified-os"
						label={t("editor.extras.verifiedOs")}
						value={verified?.os}
						onChange={(os) => setVerified({ os })}
						rows={1}
						placeholder={"windows\nlinux"}
					/>
					<EditorField id="editor-verified-game" label={t("editor.extras.verifiedGame")}>
						<TextInput
							id="editor-verified-game"
							value={verified?.game_version ?? ""}
							onChange={(game_version) => setVerified({ game_version })}
						/>
					</EditorField>
					<EditorField id="editor-verified-loader" label={t("editor.extras.verifiedLoader")}>
						<TextInput
							id="editor-verified-loader"
							value={verified?.loader_version ?? ""}
							onChange={(loader_version) => setVerified({ loader_version })}
						/>
					</EditorField>
				</div>
				{verified?.date ? (
					<div className="mt-3">
						<Button type="button" variant="ghost" size="sm" onClick={() => onChange({ ...draft, verified: undefined })}>
							{t("editor.extras.clearVerified")}
						</Button>
					</div>
				) : null}
			</div>
		</div>
	);
}

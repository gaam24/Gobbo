import { Bell, BellOff, CircleCheck, CircleOff, Download, ExternalLink, ListChecks, PackageSearch, X } from "lucide-react";
import { useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { RequirementState, SourceRequirementView } from "@/bindings";
import { HelpHint } from "@/components/app/help-hint";
import { StatusBadge } from "@/components/app/status-badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Item, ItemActions, ItemContent } from "@/components/ui/item";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { FullMod } from "@/lib/mods";
import { RequirementPlanDialog } from "@/sources/requirement-plan-dialog";
import { sourceLists } from "@/sources/requirements";
import { useSourceName } from "@/sources/source-name";

const STATE_TONE = {
	enabled: "success",
	disabled: "warning",
	missing: "danger",
} as const satisfies Record<RequirementState, "success" | "warning" | "danger">;

const STATE_ICON = {
	enabled: CircleCheck,
	disabled: CircleOff,
	missing: PackageSearch,
} as const satisfies Record<RequirementState, typeof CircleCheck>;

/// The requirements of a mod on any provider. Each row says where it stands in the profile, opens its page, and can be
/// ignored when the mod is optional or installed outside the library. Alternatives share a group, and one of them is
/// enough. The form adds a typed requirement for a provider the mod already has. Each change is sent on its own: the
/// backend merges it into the newest list.
export function SourceRequirements({ mod }: { mod: FullMod }) {
	const { t } = useTranslation();
	const sourceName = useSourceName();
	const { game, openSource, gameActions, busy } = useWorkspace();
	const providers = [...new Set([...(game?.providers ?? []).map((item) => item.id), ...mod.sources.map((source) => source.provider)])];
	const [provider, setProvider] = useState(providers[0] ?? "");
	const [id, setId] = useState("");
	const [label, setLabel] = useState("");
	const [planOpen, setPlanOpen] = useState(false);
	const requirements = mod.requirements;
	const valid =
		provider !== "" && id.trim() !== "" && !mod.sources.some((source) => source.provider === provider && source.mod_id === id.trim());

	const add = async () => {
		if (!valid) return;
		await gameActions.addRequirement(mod.id, {
			source: { provider, mod_id: id.trim(), file_id: null },
			name: label.trim(),
			from_source: false,
			group: null,
			ignored: false,
		});
		setId("");
		setLabel("");
	};

	const toggleIgnored = (item: SourceRequirementView) =>
		void gameActions.setRequirementIgnored(mod.id, item.source.provider, item.source.mod_id, !item.ignored);

	const remove = (item: SourceRequirementView) => void gameActions.removeRequirement(mod.id, item.source.provider, item.source.mod_id);

	const older = mod.source_sync.find((item) => item.older_version);

	return (
		<div className="grid gap-3">
			<div className="flex items-center justify-between gap-2">
				<div className="flex min-w-0 items-center gap-1">
					<span className="text-sm font-semibold">{t("mods.relations.requirements")}</span>
					<HelpHint>{t("mods.relations.requirementsHint")}</HelpHint>
				</div>
				<div className="flex items-center gap-2">
					{(requirements.length > 0 || mod.requires.length > 0) && (
						<Button variant="outline" size="sm" disabled={busy !== null} onClick={() => setPlanOpen(true)}>
							<ListChecks aria-hidden />
							{t("requirements.review")}
						</Button>
					)}
					{requirements.length > 0 && (
						<span className="text-subtle-foreground mt-0.5 font-mono text-xs tabular-nums">{requirements.length}</span>
					)}
				</div>
			</div>
			{older?.older_version ? (
				<p className="text-muted-foreground text-xs">
					{t("mods.relations.sourceOlder", { provider: sourceName(older.provider), version: older.older_version })}
				</p>
			) : null}
			{requirements.length > 0 ? (
				<ul className="grid gap-2">
					{sourceLists(requirements).map((list) => (
						<li
							key={list.map((item) => `${item.source.provider}:${item.source.mod_id}`).join(",")}
							className="border-border overflow-hidden rounded-lg border"
						>
							{list.length > 1 ? (
								<div className="border-border bg-muted/60 text-muted-foreground border-b px-3 py-1.5 text-xs font-medium">
									{t("mods.relations.requirementOneOf")}
								</div>
							) : null}
							<ul className="divide-line bg-muted/25 divide-y">
								{list.map((item) => (
									<RequirementRow
										key={`${item.source.provider}:${item.source.mod_id}`}
										item={item}
										onOpen={() => item.page_url && openSource(item.source.provider, item.source.mod_id)}
										onGet={
											item.state === "missing" && item.direct_download && !item.ignored
												? () => void gameActions.installRequirement(item.source, item.name)
												: undefined
										}
										getDisabled={busy !== null}
										onToggleIgnored={() => toggleIgnored(item)}
										onRemove={item.from_source ? undefined : () => remove(item)}
									/>
								))}
							</ul>
						</li>
					))}
				</ul>
			) : (
				<p className="border-border text-subtle-foreground rounded-lg border border-dashed px-3 py-3 text-xs">
					{t("mods.relations.empty")}
				</p>
			)}
			{mod.source_dlcs.length > 0 ? (
				<div className="border-border bg-muted/25 grid gap-1 rounded-lg border px-3 py-2">
					<span className="text-muted-foreground text-xs font-medium">{t("mods.relations.sourceDlcs")}</span>
					<p className="text-subtle-foreground text-xs">{t("mods.relations.sourceDlcsHint")}</p>
					<ul className="text-muted-foreground grid gap-0.5 text-xs">
						{mod.source_dlcs.flatMap(({ provider: sourceProvider, dlcs }) =>
							dlcs.map((names) => (
								<li key={`${sourceProvider}:${names.join("|")}`}>{`${sourceName(sourceProvider)}: ${names.join(" / ")}`}</li>
							)),
						)}
					</ul>
				</div>
			) : null}
			{providers.length > 0 && (
				<form
					className="border-border bg-muted/25 grid gap-2 rounded-lg border p-3"
					onSubmit={(event) => {
						event.preventDefault();
						if (valid) void add();
					}}
				>
					<span className="text-subtle-foreground text-xs">{t("mods.relations.requirementAddHint")}</span>
					<div className="grid grid-cols-[8rem_6rem_minmax(0,1fr)_auto] gap-2">
						<Field>
							<FieldLabel className="sr-only">{t("mods.relations.requirementProvider")}</FieldLabel>
							<Select value={provider} onValueChange={setProvider}>
								<SelectTrigger className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{providers.map((providerId) => (
										<SelectItem key={providerId} value={providerId}>
											{sourceName(providerId)}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</Field>
						<Field>
							<FieldLabel htmlFor="source-requirement-id" className="sr-only">
								{t("mods.relations.requirementId")}
							</FieldLabel>
							<Input
								id="source-requirement-id"
								placeholder={t("mods.relations.requirementId")}
								value={id}
								onChange={(event) => setId(event.currentTarget.value)}
							/>
						</Field>
						<Field>
							<FieldLabel htmlFor="source-requirement-name" className="sr-only">
								{t("mods.relations.requirementName")}
							</FieldLabel>
							<Input
								id="source-requirement-name"
								placeholder={t("mods.relations.requirementName")}
								value={label}
								onChange={(event) => setLabel(event.currentTarget.value)}
							/>
						</Field>
						<Button type="submit" variant="outline" size="sm" disabled={!valid}>
							{t("mods.relations.requirementAdd")}
						</Button>
					</div>
				</form>
			)}
			{planOpen && <RequirementPlanDialog folder={mod.folder} onClose={() => setPlanOpen(false)} />}
		</div>
	);
}

/// One requirement: its name, provider and id, its state in the profile, and the actions that change it.
function RequirementRow({
	item,
	onOpen,
	onGet,
	getDisabled,
	onToggleIgnored,
	onRemove,
}: {
	item: SourceRequirementView;
	onOpen: () => void;
	onGet?: () => void;
	getDisabled: boolean;
	onToggleIgnored: () => void;
	onRemove?: () => void;
}) {
	const { t } = useTranslation();
	const sourceName = useSourceName();
	const Icon = STATE_ICON[item.state];
	const name = item.name;
	const from = item.from_source
		? t("mods.relations.requirementFromSource", { provider: sourceName(item.source.provider) })
		: t("mods.relations.requirementAddedManually");

	return (
		<Item asChild size="sm" className="rounded-none border-0 px-3 py-2.5">
			<li>
				<ItemContent className="gap-0.5">
					{item.page_url ? (
						<Button
							variant="link"
							className="text-foreground h-auto min-w-0 justify-start gap-1 p-0 text-left text-sm font-medium"
							onClick={onOpen}
						>
							<span className="truncate">{name}</span>
							<ExternalLink className="text-subtle-foreground size-3 shrink-0" aria-hidden />
						</Button>
					) : (
						<span className="truncate text-sm font-medium">{name}</span>
					)}
					<span className="text-subtle-foreground font-mono text-xs">{`#${item.source.mod_id} · ${from}`}</span>
				</ItemContent>
				{item.ignored ? (
					<StatusBadge tone="neutral">
						<BellOff aria-hidden />
						{t("mods.relations.requirementIgnored")}
					</StatusBadge>
				) : (
					<StatusBadge tone={STATE_TONE[item.state]}>
						<Icon aria-hidden />
						{t(`mods.relations.requirementState.${item.state}`)}
					</StatusBadge>
				)}
				<ItemActions className="gap-0.5">
					{onGet && (
						<Button
							variant="outline"
							size="xs"
							disabled={getDisabled}
							aria-label={t("mods.relations.requirementGetName", { name })}
							title={t("mods.relations.requirementGetName", { name })}
							onClick={onGet}
						>
							<Download aria-hidden /> {t("mods.relations.requirementGet")}
						</Button>
					)}
					<Button
						variant="ghost"
						size="icon-xs"
						aria-pressed={item.ignored}
						aria-label={t(item.ignored ? "mods.relations.requirementUnignore" : "mods.relations.requirementIgnore", { name })}
						title={t(item.ignored ? "mods.relations.requirementUnignore" : "mods.relations.requirementIgnore", { name })}
						onClick={onToggleIgnored}
					>
						{item.ignored ? <Bell aria-hidden /> : <BellOff aria-hidden />}
					</Button>
					{onRemove && (
						<Button
							variant="ghost"
							size="icon-xs"
							aria-label={t("mods.relations.remove", { name })}
							title={t("mods.relations.remove", { name })}
							onClick={onRemove}
						>
							<X aria-hidden />
						</Button>
					)}
				</ItemActions>
			</li>
		</Item>
	);
}

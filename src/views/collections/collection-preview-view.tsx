import { AlertTriangle, Boxes, ExternalLink, KeyRound } from "lucide-react";
import { useMemo, useState } from "react";

import { type CollectionMode, useCollections } from "@/app/contexts/collections-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import type { CollectionPreview_Serialize, CollectionProfileView } from "@/bindings";
import { BoxList, BoxListRow, Disclosure, SectionHeading } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useTranslation } from "@/i18n";
import { defaultSelection, downloadSizeKb, orderedItems, revisionLabel } from "@/lib/collections";
import { translate } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { CollectionMemberList } from "@/views/collections/collection-member-list";
import { CollectionTitle, collectionMeta } from "@/views/collections/collection-title";

const NEW_PROFILE = "__new__";

/// The preview before an install: what the collection is, its members with their status and the selection, and where
/// and how it installs.
export function CollectionPreviewView({ preview, onClose }: { preview: CollectionPreview_Serialize; onClose: () => void }) {
	const { t, i18n } = useTranslation();
	const { openAppSettings } = useNavigation();
	const { busy, failure, start, openPage } = useCollections();
	const plan = preview.plan;
	const items = useMemo(() => orderedItems(plan), [plan]);
	const [selected, setSelected] = useState<Set<string>>(() => new Set(defaultSelection(plan)));
	const installed = preview.already_installed;
	const [profile, setProfile] = useState(installed ? preview.profile : NEW_PROFILE);
	const [name, setName] = useState(plan.info.name);
	const [mode, setMode] = useState<CollectionMode>(preview.direct_download ? "automatic" : "guided");
	const [position, setPosition] = useState<"bottom" | "top">("bottom");
	const active = preview.profiles.find((known) => known.active);
	const keyMissing = preview.needs_key && !preview.key_saved;
	const requiredOff = plan.items.filter((item) => !(item.optional || selected.has(item.key))).length;
	const optionalCount = plan.items.filter((item) => item.optional).length;
	const download = downloadSizeKb(plan.items, preview.statuses, selected);
	const unsupported = plan.issues.filter((issue) => issue.severity === "unsupported").length;

	let modeHint = t("collection.mode.premiumOnly");
	if (mode === "automatic") modeHint = t("collection.mode.automaticHint");
	else if (preview.direct_download) modeHint = t("collection.mode.guidedHint");
	const toggle = (key: string, checked: boolean) =>
		setSelected((current) => {
			const next = new Set(current);
			if (checked) next.add(key);
			else next.delete(key);
			return next;
		});
	const install = () =>
		void start(
			{
				link: plan.info.url,
				revision: plan.info.revision,
				profile: profile === NEW_PROFILE ? null : profile,
				profile_name: name,
				selected: [...selected],
				position,
				activate: profile === NEW_PROFILE,
			},
			mode,
		);
	const profileName = (item: CollectionProfileView) => (item.pack ? t("collection.profile.pack", { name: item.name }) : item.name);

	return (
		<ModalDialog
			width="3xl"
			className="h-[85vh]"
			title={
				<CollectionTitle name={plan.info.name} revision={revisionLabel(plan.info)} provider={plan.info.provider} adult={plan.info.adult}>
					{plan.limited && <Badge variant="outline">{t("collection.limited")}</Badge>}
				</CollectionTitle>
			}
			description={collectionMeta(t, i18n.language, plan.info, [preview.account])}
			onClose={onClose}
		>
			<div className="flex h-full min-h-0 flex-col gap-4 text-sm">
				{plan.info.summary && <p className="text-muted-foreground max-w-[65ch] text-pretty">{plan.info.summary}</p>}
				{keyMissing && (
					<Alert variant="destructive">
						<KeyRound />
						<AlertTitle>{t("collection.noKey")}</AlertTitle>
						<AlertDescription>
							<Button size="sm" variant="outline" onClick={() => openAppSettings()}>
								{t("collection.openSettings")}
							</Button>
						</AlertDescription>
					</Alert>
				)}
				{installed && (
					<Alert>
						<Boxes />
						<AlertDescription>{t("collection.alreadyInstalled")}</AlertDescription>
					</Alert>
				)}
				{plan.issues.length > 0 && (
					<Alert variant={unsupported > 0 ? "destructive" : "default"}>
						<AlertTriangle />
						<AlertTitle>{t("collection.issues", { count: plan.issues.length })}</AlertTitle>
						<AlertDescription>
							<Disclosure title={t("collection.issuesShow")}>
								<BoxList scroll className="max-h-32">
									{[...new Set(plan.issues.map((issue) => translate(issue.message)))].map((text) => (
										<BoxListRow key={text}>{text}</BoxListRow>
									))}
								</BoxList>
							</Disclosure>
						</AlertDescription>
					</Alert>
				)}
				{(plan.info.description || plan.info.instructions) && (
					<Disclosure title={t("collection.description")}>
						<div className="text-muted-foreground grid max-h-48 gap-3 overflow-auto leading-relaxed whitespace-pre-wrap">
							{plan.info.description && <p className="max-w-[65ch]">{plan.info.description}</p>}
							{plan.info.instructions && (
								<div>
									<p className="text-foreground font-medium">{t("collection.instructions")}</p>
									<p className="max-w-[65ch]">{plan.info.instructions}</p>
								</div>
							)}
						</div>
					</Disclosure>
				)}

				<div className="flex min-h-64 flex-1 flex-col gap-2">
					<SectionHeading
						title={t("collection.membersTitle")}
						actions={
							<>
								<span className="text-muted-foreground mr-1 font-mono text-xs tabular-nums">
									{t("collection.selected", { count: selected.size, total: plan.items.length })}
								</span>
								<Button size="xs" variant="ghost" onClick={() => setSelected(new Set(plan.items.map((item) => item.key)))}>
									{t("collection.selectAll")}
								</Button>
								{optionalCount > 0 && (
									<Button size="xs" variant="ghost" onClick={() => setSelected(new Set(defaultSelection(plan)))}>
										{t("collection.selectRequired")}
									</Button>
								)}
							</>
						}
					/>
					<CollectionMemberList
						items={items}
						statuses={preview.statuses}
						provider={plan.info.provider}
						selected={selected}
						onToggle={toggle}
						className="flex-1 grid-rows-[auto_minmax(0,1fr)]"
					/>
					{requiredOff > 0 && (
						<p className="text-draft flex items-center gap-1.5 text-xs">
							<AlertTriangle className="size-3.5" /> {t("collection.requiredSkipped", { count: requiredOff })}
						</p>
					)}
				</div>

				<div className="grid gap-3 rounded-lg border p-3 sm:grid-cols-2">
					<Field>
						<FieldLabel htmlFor="collection-profile">{t("collection.profile.label")}</FieldLabel>
						<Select value={profile} onValueChange={setProfile}>
							<SelectTrigger id="collection-profile" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value={NEW_PROFILE}>{t("collection.profile.new")}</SelectItem>
								{preview.profiles.map((item) => (
									<SelectItem key={item.id} value={item.id} disabled={item.pack}>
										{profileName(item)}
										{item.collections > 0 && (
											<span className="text-muted-foreground text-xs">
												{` · ${t("collection.profile.collections", { count: item.collections })}`}
											</span>
										)}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						{profile === NEW_PROFILE && (
							<FieldDescription>
								{active ? t("collection.profile.newHint", { name: active.name }) : t("collection.profile.newHintEmpty")}
							</FieldDescription>
						)}
					</Field>
					{profile === NEW_PROFILE ? (
						<Field>
							<FieldLabel htmlFor="collection-profile-name">{t("collection.profile.name")}</FieldLabel>
							<Input id="collection-profile-name" value={name} onChange={(event) => setName(event.currentTarget.value)} />
						</Field>
					) : (
						<Field>
							<FieldLabel htmlFor="collection-position">{t("collection.position.label")}</FieldLabel>
							<Select value={position} onValueChange={(value) => setPosition(value === "top" ? "top" : "bottom")}>
								<SelectTrigger id="collection-position" className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="bottom">{t("collection.position.bottom")}</SelectItem>
									<SelectItem value="top">{t("collection.position.top")}</SelectItem>
								</SelectContent>
							</Select>
							<FieldDescription>{t("collection.position.hint")}</FieldDescription>
						</Field>
					)}
					<Field className="sm:col-span-2">
						<FieldLabel>{t("collection.mode.label")}</FieldLabel>
						<ToggleGroup
							type="single"
							variant="outline"
							value={mode}
							onValueChange={(value) => value && setMode(value === "automatic" ? "automatic" : "guided")}
							aria-label={t("collection.mode.label")}
						>
							<ToggleGroupItem value="automatic" disabled={!preview.direct_download}>
								{t("collection.mode.automatic")}
							</ToggleGroupItem>
							<ToggleGroupItem value="guided">{t("collection.mode.guided")}</ToggleGroupItem>
						</ToggleGroup>
						<FieldDescription>{modeHint}</FieldDescription>
					</Field>
				</div>
				{failure && (
					<p role="alert" className="text-destructive text-sm">
						{failure}
					</p>
				)}
			</div>
			<DialogFooter className="items-center">
				<span className="text-muted-foreground mr-auto font-mono text-xs tabular-nums">
					{download > 0
						? t("collection.footer.download", { count: selected.size, size: formatSize(download, i18n.language) })
						: t("collection.footer.selected", { count: selected.size })}
				</span>
				{installed && (
					<Button variant="ghost" onClick={() => void openPage(preview.profile, plan.info)}>
						<ExternalLink /> {t("collection.panel.openPage")}
					</Button>
				)}
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={keyMissing || busy || selected.size === 0} onClick={install}>
					<Boxes /> {installed ? t("collection.repair") : t("collection.install.start")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

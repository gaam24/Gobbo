import { Download } from "lucide-react";
import { useState } from "react";
import type { CatalogEntryView, CatalogStatus } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { StatusBadge } from "@/components/app/status-badge";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DialogFooter } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTranslation } from "@/i18n";
import type { CatalogInstallFailure, CatalogInstallReport, CatalogProgress } from "@/views/games/use-catalog";

/// One catalog entry. A blocked entry keeps its row visible but cannot be selected; the reason is written next to it,
/// so the disabled state is never the only signal.
function EntryRow({
	entry,
	checked,
	blocked,
	onCheckedChange,
}: {
	entry: CatalogEntryView;
	checked: boolean;
	blocked: boolean;
	onCheckedChange: (checked: boolean) => void;
}) {
	const { t } = useTranslation();
	return (
		<BoxListRow>
			<Checkbox
				id={`catalog-${entry.id}`}
				aria-label={t("catalog.select", { name: entry.name })}
				checked={checked}
				disabled={blocked}
				onCheckedChange={(value) => onCheckedChange(value === true)}
			/>
			<div className="grid min-w-0 gap-0.5">
				<span className="flex min-w-0 flex-wrap items-center gap-1.5">
					<span className="truncate font-medium">{entry.name}</span>
					{entry.verified && <StatusBadge tone="success">{t("games.verified")}</StatusBadge>}
				</span>
				<span className="text-muted-foreground font-mono text-xs">{entry.id}</span>
				{entry.loader && <span className="text-muted-foreground text-xs">{t("catalog.loader", { loader: entry.loader })}</span>}
				{blocked && (
					<span className="text-xs">
						{entry.min_gobbo ? t("catalog.requiresVersion", { version: entry.min_gobbo }) : t("catalog.requiresNewer")}
					</span>
				)}
			</div>
		</BoxListRow>
	);
}

function EntryList({
	entries,
	selected,
	blocked = false,
	emptyText,
	onToggle,
}: {
	entries: CatalogEntryView[];
	selected: Set<string>;
	blocked?: boolean;
	emptyText: string;
	onToggle: (id: string, checked: boolean) => void;
}) {
	if (entries.length === 0) return <p className="text-muted-foreground text-sm">{emptyText}</p>;
	return (
		<BoxList scroll>
			{entries.map((entry) => (
				<EntryRow
					key={entry.id}
					entry={entry}
					checked={selected.has(entry.id)}
					blocked={blocked}
					onCheckedChange={(checked) => onToggle(entry.id, checked)}
				/>
			))}
		</BoxList>
	);
}

const ignoreToggle = () => {
	// Blocked rows have no selection to change.
};

function toggle(current: Set<string>, change: (next: Set<string>) => void, id: string, checked: boolean) {
	const next = new Set(current);
	if (checked) next.add(id);
	else next.delete(id);
	change(next);
}

/// The only catalog install UI. All commands stay in `useCatalog`; this dialog only draws the two lists, the shared
/// progress and the result of the download. Updates start checked, new games unchecked.
export function CatalogDialog({
	status,
	progress,
	onInstall,
	onClose,
}: {
	status: CatalogStatus | null;
	progress: CatalogProgress;
	onInstall: (ids: string[]) => Promise<CatalogInstallReport>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [updates, setUpdates] = useState<Set<string>>(() => new Set(status?.changed.map((entry) => entry.id) ?? []));
	const [available, setAvailable] = useState<Set<string>>(() => new Set<string>());
	const [failures, setFailures] = useState<CatalogInstallFailure[]>([]);
	const [busy, setBusy] = useState(false);
	const installing = progress.kind !== "idle";
	const selected = [...updates, ...available];

	const download = async () => {
		// The ids belong to this click; a later selection change must not retarget an in-flight install.
		const ids = [...updates, ...available];
		setBusy(true);
		setFailures([]);
		const report = await onInstall(ids);
		if (report.installed.length > 0) {
			// A partial success leaves only the refused ids checked, so a second click retries those and not the
			// definitions that are already installed.
			const done = new Set(report.installed);
			setUpdates((current) => new Set([...current].filter((id) => !done.has(id))));
			setAvailable((current) => new Set([...current].filter((id) => !done.has(id))));
		}
		// A partial failure stays open with the reasons listed, like the definition import dialog does.
		if (report.failures.length > 0) setFailures(report.failures);
		else onClose();
		setBusy(false);
	};

	return (
		<ModalDialog title={t("catalog.title")} description={t("catalog.hint")} width="lg" onClose={onClose}>
			<Tabs defaultValue="updates">
				<TabsList>
					<TabsTrigger value="updates">{t("catalog.tab.updates")}</TabsTrigger>
					<TabsTrigger value="available">{t("catalog.tab.available")}</TabsTrigger>
				</TabsList>
				<TabsContent value="updates" className="grid gap-3">
					<EntryList
						entries={status?.changed ?? []}
						selected={updates}
						emptyText={t("catalog.emptyUpdates")}
						onToggle={(id, checked) => toggle(updates, setUpdates, id, checked)}
					/>
					{(status?.blocked.length ?? 0) > 0 && (
						<EntryList entries={status?.blocked ?? []} selected={new Set()} blocked emptyText="" onToggle={ignoreToggle} />
					)}
				</TabsContent>
				<TabsContent value="available" className="grid gap-3">
					<EntryList
						entries={status?.available ?? []}
						selected={available}
						emptyText={t("catalog.emptyAvailable")}
						onToggle={(id, checked) => toggle(available, setAvailable, id, checked)}
					/>
				</TabsContent>
			</Tabs>

			{installing && (
				<div className="grid gap-2">
					<Progress
						value={progress.kind === "downloading" ? progress.percent : undefined}
						aria-label={progress.kind === "installing" ? t("catalog.installing") : t("catalog.downloading")}
						aria-valuetext={progress.kind === "downloading" && progress.percent !== undefined ? `${progress.percent}%` : undefined}
					/>
					<p className="text-muted-foreground text-sm">
						{progress.kind === "installing" ? t("catalog.installing") : t("catalog.downloading")}
					</p>
				</div>
			)}

			{failures.length > 0 && (
				<Alert variant="destructive">
					<AlertTitle>{t("catalog.failedTitle")}</AlertTitle>
					<AlertDescription>
						<ul className="grid gap-1">
							{failures.map((failure) => (
								<li key={failure.id}>
									<span className="font-mono text-xs">{failure.id}</span> {failure.message}
								</li>
							))}
						</ul>
					</AlertDescription>
				</Alert>
			)}

			<DialogFooter>
				<Button variant="outline" disabled={installing} onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={selected.length === 0 || installing || busy} onClick={() => void download()}>
					{installing ? <Spinner aria-hidden /> : <Download aria-hidden />}
					{t("catalog.download")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}

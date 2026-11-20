import { Eye, Search } from "lucide-react";
import { type ReactNode, useDeferredValue, useRef, useState } from "react";

import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands } from "@/bindings";
import { PathText } from "@/components/app/path-text";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { foldText } from "@/lib/fold-text";
import { unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";
import { useVirtualWindow } from "@/lib/use-virtual-window";
import { ConflictPreviewPanel } from "@/views/conflict-explorer/preview";

/// The fixed height of a path row, matching the classes below. Only the rows around the viewport are mounted, so a
/// profile with thousands of shared paths still scrolls without laying out the whole list.
const ROW_HEIGHT = 64;
const AUTO = "auto";

/// Every shared file of the active profile, in path order. The winner and the overridden copies are the same plan data
/// the status counts, so the total cannot disagree with the header. A path can be given to another mod from here.
export function ConflictExplorerDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t, i18n } = useTranslation();
	const { status, gameActions } = useWorkspace();
	const [version, setVersion] = useState(0);
	const [query, setQuery] = useState("");
	const [copies, setCopies] = useState("all");
	const [busy, setBusy] = useState<string | null>(null);
	const [preview, setPreview] = useState<{
		path: string;
		families: Array<{ id: string; name: string }>;
	} | null>(null);
	const viewport = useRef<HTMLDivElement>(null);
	const deferred = useDeferredValue(query);
	const explorer = useLoad(`conflicts:${gameId}:${version}`, () => unwrap(commands.conflictExplorer(gameId)));

	const resetScroll = () => {
		if (viewport.current) viewport.current.scrollTop = 0;
	};

	const names = new Map((status?.mods ?? []).map((mod) => [mod.id, mod.info.name || mod.folder]));
	const nameOf = (id: string) => names.get(id) ?? id;
	const needle = foldText(deferred.trim());
	const filtered = (explorer?.entries ?? []).filter((entry) => {
		const copyMatch = copies === "all" || (copies === "two" ? entry.copies === 2 : entry.copies > 2);

		if (!copyMatch) return false;

		if (needle === "") return true;
		return (
			foldText(entry.path).includes(needle) ||
			foldText(nameOf(entry.winner)).includes(needle) ||
			entry.losers.some((loser) => foldText(nameOf(loser)).includes(needle))
		);
	});

	const { items: virtualRows, totalSize } = useVirtualWindow({
		count: filtered.length,
		getScrollElement: () => viewport.current,
		estimateSize: () => ROW_HEIGHT,
		overscan: 6,
		// The viewport mounts with the data, so give the first render a window before it can be measured.
		initialRect: { width: 0, height: 480 },
	});

	const give = async (path: string, family: string) => {
		setBusy(path);
		await gameActions.setFileWinner(path, family === AUTO ? null : family);
		setBusy(null);
		setVersion((count) => count + 1);
	};

	let body: ReactNode;

	if (preview) {
		body = <ConflictPreviewPanel gameId={gameId} path={preview.path} families={preview.families} onBack={() => setPreview(null)} />;
	} else if (!explorer) {
		body = <p className="text-muted-foreground text-sm">{t("conflicts.loading")}</p>;
	} else if (explorer.entries.length === 0) {
		body = <p className="text-muted-foreground text-sm">{t("conflicts.empty")}</p>;
	} else if (filtered.length === 0) {
		body = <p className="text-muted-foreground text-sm">{t("conflicts.noMatch")}</p>;
	} else {
		body = (
			<div ref={viewport} className="h-[55vh] overflow-y-auto rounded-lg border">
				<div className="relative" style={{ height: totalSize }}>
					{virtualRows.map((item) => {
						const entry = filtered[item.index];

						if (!entry) return null;
						return (
							<div
								key={entry.path}
								className="border-b px-3 py-2"
								style={{
									position: "absolute",
									top: item.start,
									left: 0,
									right: 0,
									height: item.size,
								}}
							>
								<div className="flex items-center gap-3">
									<div className="grid min-w-0 flex-1 gap-0.5">
										<PathText path={entry.path} />
										<span className="text-muted-foreground truncate text-xs">
											{t("conflicts.overrides", {
												winner: nameOf(entry.winner),
												losers: entry.losers.map(nameOf).join(", "),
												size: formatSize(Math.ceil(entry.bytes / 1024), i18n.language),
											})}
										</span>
									</div>
									<Button
										variant="ghost"
										size="icon"
										className="size-8 shrink-0"
										aria-label={t("conflicts.preview")}
										title={t("conflicts.preview")}
										onClick={() =>
											setPreview({
												path: entry.path,
												families: [entry.winner, ...entry.losers].map((id) => ({
													id,
													name: nameOf(id),
												})),
											})
										}
									>
										<Eye />
									</Button>
									<Select value={entry.winner} disabled={busy !== null} onValueChange={(family) => void give(entry.path, family)}>
										<SelectTrigger className="h-8 w-44 text-xs" aria-label={t("conflicts.giveTo")}>
											<SelectValue />
										</SelectTrigger>
										<SelectContent>
											<SelectItem value={AUTO}>{t("conflicts.auto")}</SelectItem>
											{[entry.winner, ...entry.losers].map((family) => (
												<SelectItem key={family} value={family}>
													{nameOf(family)}
												</SelectItem>
											))}
										</SelectContent>
									</Select>
								</div>
							</div>
						);
					})}
				</div>
			</div>
		);
	}

	return (
		<ModalDialog title={t("conflicts.title")} description={t("conflicts.hint")} width="3xl" onClose={onClose}>
			<div className="grid gap-3">
				{!preview && (
					<div className="flex flex-wrap items-center gap-2">
						<div className="relative min-w-48 flex-1">
							<Search className="text-muted-foreground pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2" aria-hidden />
							<Input
								className="pl-8"
								value={query}
								placeholder={t("conflicts.search")}
								aria-label={t("conflicts.search")}
								onChange={(event) => {
									setQuery(event.currentTarget.value);
									resetScroll();
								}}
							/>
						</div>
						<Select
							value={copies}
							onValueChange={(value) => {
								setCopies(value);
								resetScroll();
							}}
						>
							<SelectTrigger className="w-44" aria-label={t("conflicts.copies")}>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">{t("conflicts.allCopies")}</SelectItem>
								<SelectItem value="two">{t("conflicts.twoCopies")}</SelectItem>
								<SelectItem value="more">{t("conflicts.moreCopies")}</SelectItem>
							</SelectContent>
						</Select>
						<span className="text-muted-foreground text-sm tabular-nums">
							{t("conflicts.showing", {
								shown: filtered.length,
								total: explorer?.total ?? 0,
							})}
						</span>
					</div>
				)}
				{body}
			</div>
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

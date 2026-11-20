import { useState } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { commands, type MountDiagnostics } from "@/bindings";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { type TranslationKey, useTranslation } from "@/i18n";
import { messageOf, translate, unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";

const REASON: Record<string, TranslationKey> = {
	profile: "console.mountsReasonProfile",
	set: "console.mountsReasonSet",
	recovery: "console.mountsReasonRecovery",
};

/// The read-only part of the panel. It owns no command, so a test can render it with a fixture.
export function MountsView({
	view,
	selected,
	onToggle,
	onClean,
}: {
	view: MountDiagnostics;
	selected: ReadonlySet<string>;
	onToggle: (path: string, checked: boolean) => void;
	onClean: () => void;
}) {
	const { t, i18n } = useTranslation();
	return (
		<div className="grid gap-6">
			<section aria-labelledby="console-mounts-active" className="grid gap-2">
				<h3 id="console-mounts-active" className="text-sm font-medium">
					{t("console.mountsActive")}
				</h3>
				{view.mounts.length === 0 ? (
					<p className="text-muted-foreground text-xs">{t("console.mountsEmpty")}</p>
				) : (
					<ul className="grid gap-1 font-mono text-xs break-all">
						{view.mounts.map((mount) => (
							<li key={mount.id} className="flex flex-wrap gap-2">
								<span className="font-medium">{mount.id}</span>
								<span className="text-muted-foreground">{mount.guest}</span>
								<span>
									{"→ "}
									{mount.upper}
								</span>
								<span className={mount.active ? "text-destructive" : "text-muted-foreground"}>
									{mount.active ? t("console.mountsLive") : t("console.mountsIdle")}
								</span>
							</li>
						))}
					</ul>
				)}
			</section>

			{view.warnings.length > 0 && (
				<section aria-labelledby="console-mounts-warnings" className="grid gap-2">
					<h3 id="console-mounts-warnings" className="text-destructive text-sm font-medium">
						{t("console.mountsWarnings")}
					</h3>
					<ul className="grid gap-1 text-xs">
						{view.warnings.map((warning) => (
							<li key={warning.code} className="text-destructive">
								{translate(warning)}
							</li>
						))}
					</ul>
				</section>
			)}

			<section aria-labelledby="console-mounts-orphans" className="grid gap-2">
				<h3 id="console-mounts-orphans" className="text-sm font-medium">
					{t("console.mountsOrphans")}
				</h3>
				<p className="text-muted-foreground text-xs">{t("console.mountsOrphansHint")}</p>
				{view.orphans.length === 0 ? (
					<p className="text-muted-foreground text-xs">{t("console.mountsOrphansEmpty")}</p>
				) : (
					<ul className="grid gap-2">
						{view.orphans.map((orphan) => {
							const id = `console-orphan-${orphan.path}`;
							return (
								<li key={orphan.path} className="flex items-start gap-2">
									<Checkbox
										id={id}
										checked={selected.has(orphan.path)}
										onCheckedChange={(checked) => onToggle(orphan.path, checked === true)}
									/>
									<label htmlFor={id} className="grid min-w-0 gap-0.5 font-mono text-xs break-all">
										<span>{orphan.path}</span>
										<span className="text-muted-foreground">
											{`${t(REASON[orphan.reason] ?? "console.mountsReasonOther")} · ${formatSize(Math.round(orphan.size_bytes / 1024), i18n.language)}`}
										</span>
									</label>
								</li>
							);
						})}
					</ul>
				)}
				<div>
					<Button variant="outline" size="sm" disabled={selected.size === 0} onClick={onClean}>
						{t("console.mountsClean")}
					</Button>
				</div>
			</section>
		</div>
	);
}

/// Loads the diagnostics for one game and asks for an explicit confirmation before any folder is deleted.
export function MountsPanel({ gameId }: { gameId: string }) {
	const { t } = useTranslation();
	const { notify, attempt } = useNoticeActions();
	const [reloadKey, setReloadKey] = useState(0);
	const [failure, setFailure] = useState<string | null>(null);
	const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
	const [confirming, setConfirming] = useState(false);
	const view = useLoad(`mounts-${gameId}`, () => unwrap(commands.mountDiagnostics(gameId)), {
		reloadKey,
		onError: (error) => setFailure(messageOf(error)),
	});

	const toggle = (path: string, checked: boolean) => {
		const next = new Set(selected);

		if (checked) next.add(path);
		else next.delete(path);
		setSelected(next);
	};

	const clean = async () => {
		setConfirming(false);
		const paths = [...selected];

		if (paths.length === 0) return;
		const ok = await attempt(async () => {
			await unwrap(commands.cleanOrphanLayers(gameId, paths));
			notify(t("console.mountsCleanDone"));
		});

		if (!ok) return;
		setSelected(new Set());
		setReloadKey((key) => key + 1);
	};

	return (
		<div className="grid gap-3">
			{failure && <p className="text-destructive text-xs">{failure}</p>}
			{view ? (
				<MountsView view={view} selected={selected} onToggle={toggle} onClean={() => setConfirming(true)} />
			) : (
				<p className="text-muted-foreground text-xs">{t("console.mountsLoading")}</p>
			)}
			<ConfirmDialog
				open={confirming}
				onOpenChange={setConfirming}
				title={t("console.mountsCleanTitle")}
				description={t("console.mountsCleanBody")}
				confirmLabel={t("console.mountsClean")}
				onConfirm={() => void clean()}
			>
				<ul className="grid gap-1 font-mono text-xs break-all">
					{[...selected].map((path) => (
						<li key={path}>{path}</li>
					))}
				</ul>
			</ConfirmDialog>
		</div>
	);
}

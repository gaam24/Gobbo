import { Download } from "lucide-react";
import type { LoaderChoice, LoaderCopyView, LoaderSetupView } from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { StatusBadge } from "@/components/app/status-badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { formatSize } from "@/lib/mods";

/// The loader package, its source and the copies in the library. The official release is offered, never forced: a build
/// a mod site publishes for this game (often newer, or patched for it) is a copy the user can pick just as well. The one
/// the user picks becomes the only enabled copy; the overlay serves it like every other mod and the game folder is not
/// written.
export function LoaderCard({
	loader,
	installing,
	onInstall,
}: {
	loader: LoaderSetupView;
	installing: boolean;
	onInstall: (choice: LoaderChoice) => void;
}) {
	const { t, i18n } = useTranslation();
	let tone: "success" | "update" | "warning" = "warning";
	let state = t("setupCheck.loader.missing");

	if (loader.state === "installed") {
		tone = "success";
		state = t("setupCheck.loader.installed");
	} else if (loader.state === "update") {
		tone = "update";
		state = t("setupCheck.loader.update");
	}

	const active = loader.copies.find((copy) => copy.enabled);
	let installed = loader.installed_version;

	if (installed === null && active) {
		installed = active.name;
	} else if (installed === null && loader.state === "installed") {
		installed = t("setupCheck.loader.manual");
	}

	const size = loader.size > 0 ? formatSize(Math.ceil(loader.size / 1024), i18n.language) : null;
	// A manual install in the game folder needs no button; any other state offers the official release unless it is
	// already the copy in use.
	const manual = loader.state === "installed" && !active;
	const officialInUse = active?.official === true;
	let officialLabel = t("setupCheck.loader.installAction", { name: loader.name, version: loader.version });

	if (loader.state === "update") {
		officialLabel = t("setupCheck.loader.updateAction", { name: loader.name, version: loader.version });
	} else if (loader.copies.some((copy) => copy.official)) {
		officialLabel = t("setupCheck.loader.useOfficialAction", { name: loader.name, version: loader.version });
	}

	return (
		<section className="grid gap-2 rounded-lg border p-3" aria-labelledby="setup-loader-title">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<div className="grid gap-0.5">
					<h3 id="setup-loader-title" className="text-sm font-semibold">
						{loader.name}
					</h3>
					<p className="text-muted-foreground text-xs">{t("setupCheck.loader.hint", { name: loader.name })}</p>
				</div>
				<StatusBadge tone={tone}>{state}</StatusBadge>
			</div>
			{installed && (
				<p className="text-sm">
					<span className="text-muted-foreground">{t("setupCheck.loader.inUseLabel")}</span> {installed}
				</p>
			)}
			{loader.copies.length > 0 && (
				<BoxList>
					{loader.copies.map((copy) => (
						<LoaderCopyRow
							key={copy.folder}
							copy={copy}
							installing={installing}
							onUse={() => onInstall({ kind: "library", folder: copy.folder })}
						/>
					))}
				</BoxList>
			)}
			<Disclosure title={t("setupCheck.loader.official", { version: loader.version })}>
				<dl className="grid gap-1 text-xs sm:grid-cols-[auto_1fr] sm:gap-x-3">
					<dt className="text-muted-foreground">{t("setupCheck.loader.source")}</dt>
					<dd className="truncate font-mono" title={loader.source}>
						{loader.source}
					</dd>
					<dt className="text-muted-foreground">{t("setupCheck.loader.hash")}</dt>
					<dd className="truncate font-mono" title={loader.sha256}>
						{loader.sha256.slice(0, 16)}
					</dd>
					{size && (
						<>
							<dt className="text-muted-foreground">{t("setupCheck.loader.size")}</dt>
							<dd className="tabular-nums">{size}</dd>
						</>
					)}
					<dt className="text-muted-foreground">{t("setupCheck.loader.target")}</dt>
					<dd className="font-mono break-all">{loader.target === "" ? t("setupCheck.loader.gameFolder") : loader.target}</dd>
				</dl>
			</Disclosure>
			{!(manual || officialInUse) && (
				<Button
					type="button"
					variant={active ? "ghost" : "outline"}
					size="sm"
					className="w-fit"
					disabled={installing}
					onClick={() => onInstall({ kind: "official" })}
				>
					{installing ? <Spinner aria-hidden /> : <Download />}
					{officialLabel}
				</Button>
			)}
		</section>
	);
}

/// One copy of the loader in the library, with the button that makes it the copy in use.
function LoaderCopyRow({ copy, installing, onUse }: { copy: LoaderCopyView; installing: boolean; onUse: () => void }) {
	const { t } = useTranslation();
	return (
		<BoxListRow className="flex-nowrap items-center gap-2">
			<div className="grid min-w-0 flex-1 gap-0.5">
				<p className="truncate text-sm" title={copy.name}>
					{copy.name}
					{copy.version && <span className="text-muted-foreground"> {copy.version}</span>}
				</p>
				<p className="text-muted-foreground text-xs">
					{copy.official ? t("setupCheck.loader.copyOfficial") : t("setupCheck.loader.copyOther")}
				</p>
			</div>
			{copy.enabled ? (
				<StatusBadge tone="success">{t("setupCheck.loader.inUse")}</StatusBadge>
			) : (
				<Button type="button" variant="outline" size="sm" disabled={installing} onClick={onUse}>
					{t("setupCheck.loader.useAction")}
				</Button>
			)}
		</BoxListRow>
	);
}

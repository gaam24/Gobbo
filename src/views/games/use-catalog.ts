import { useCallback, useRef, useState } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { type CatalogInstallOutcome, type CatalogInstallStep, type CatalogStatus, commands } from "@/bindings";
import { useTranslation } from "@/i18n";
import { codeOf, detailOf, errorText, messageOf, unwrap } from "@/lib/ipc";
import { newProgressChannel } from "@/lib/tauri";

/// The install progress as the dialog draws it. `downloading` without a percent is an indeterminate bar.
export type CatalogProgress = { kind: "idle" } | { kind: "downloading"; percent?: number } | { kind: "installing" };

export interface CatalogInstallFailure {
	id: string;
	message: string;
}

export interface CatalogInstallReport {
	installed: string[];
	failures: CatalogInstallFailure[];
}

/// Whether the banner has anything to offer. Blocked entries are shown in the dialog, not counted in the banner.
export function hasCatalogUpdates(status: CatalogStatus | null): boolean {
	return status !== null && (status.changed.length > 0 || status.available.length > 0);
}

/// The progress state one channel step produces. Kept pure so the transition is checked without a Tauri runtime.
export function progressOf(step: CatalogInstallStep): CatalogProgress {
	return step.step === "downloading" ? { kind: "downloading", percent: step.percent ?? undefined } : { kind: "installing" };
}

/// One record per refused id. The backend sends an `AppError` with a stable code, so `errorText` translates it.
export function installFailures(outcomes: CatalogInstallOutcome[]): CatalogInstallFailure[] {
	return outcomes.flatMap((outcome) =>
		outcome.result.status === "failed" ? [{ id: outcome.id, message: errorText(outcome.result.error) }] : [],
	);
}

/// Owns the official catalog for the Games page: the check, its status and the install with its progress. `onInstalled`
/// is the games reload the page already uses, so a downloaded definition shows up without a restart.
export function useCatalog({ onInstalled }: { onInstalled: () => void | Promise<void> }) {
	const { t } = useTranslation();
	const { notify, notifyError } = useNoticeActions();
	const [status, setStatus] = useState<CatalogStatus | null>(null);
	const [checking, setChecking] = useState(false);
	const [progress, setProgress] = useState<CatalogProgress>({ kind: "idle" });
	const installing = useRef(false);

	/// A silent failure leaves the page as it is; a manual check reports it. An unsupported api version is always
	/// reported, because the page cannot offer anything until the app is updated.
	const check = useCallback(
		async (silent = false) => {
			setChecking(true);
			try {
				const fresh = await unwrap(commands.catalogCheck());
				setStatus(fresh);
				if (!(silent || hasCatalogUpdates(fresh))) notify(t("catalog.upToDate"));
			} catch (failure) {
				if (!silent || codeOf(failure) === "catalog.unsupported") {
					notifyError(messageOf(failure), detailOf(failure));
				}
			}
			setChecking(false);
		},
		[notify, notifyError, t],
	);

	const install = useCallback(
		async (ids: string[]): Promise<CatalogInstallReport> => {
			// Capture the call's own data before the first await; a later render must not change what is installed.
			const requested = [...ids];
			const reload = onInstalled;
			if (requested.length === 0 || installing.current) return { installed: [], failures: [] };
			installing.current = true;
			setProgress({ kind: "downloading" });
			const channel = newProgressChannel<CatalogInstallStep>((step) => setProgress(progressOf(step)));
			let report: CatalogInstallReport;

			try {
				const outcomes = await unwrap(commands.catalogInstall(requested, channel));
				const failures = installFailures(outcomes);
				for (const failure of failures) notifyError(failure.message);
				if (failures.length < outcomes.length) {
					await reload();
					await check(true);
				}
				report = {
					installed: outcomes.flatMap((outcome) => (outcome.result.status === "installed" ? [outcome.id] : [])),
					failures,
				};
			} catch (failure) {
				const message = messageOf(failure);
				notifyError(message, detailOf(failure));
				report = { installed: [], failures: [{ id: requested.join(", "), message }] };
			}

			setProgress({ kind: "idle" });
			installing.current = false;
			return report;
		},
		[check, notifyError, onInstalled],
	);

	return { status, checking, progress, check, install };
}

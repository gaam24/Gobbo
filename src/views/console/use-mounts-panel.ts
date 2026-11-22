import { useState } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { commands } from "@/bindings";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// Loads one game's mount diagnostics and asks for an explicit confirmation before any orphan folder is deleted.
export function useMountsPanel(gameId: string) {
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

	return { failure, view, selected, confirming, setConfirming, toggle, clean };
}

import { useState } from "react";
import { commands, type VersionView } from "@/bindings";
import { unwrap } from "@/lib/ipc";

/// The installed versions of one mod, read only when the picker opens: the status list carries only the version count,
/// so a library of thousands of mods does not ship every version of every mod on each status.
export function useModVersions(gameId: string, folder: string) {
	const [versions, setVersions] = useState<VersionView[] | null>(null);
	const [failed, setFailed] = useState(false);
	const loadVersions = async () => {
		// A failed read is retried on the next open; a successful one stays until the row remounts.
		if (versions !== null) return;
		setFailed(false);

		try {
			setVersions(await unwrap(commands.modVersions(gameId, folder)));
		} catch {
			setFailed(true);
		}
	};
	return { versions, failed, loadVersions };
}

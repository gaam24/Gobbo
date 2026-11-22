import { useState } from "react";
import { commands } from "@/bindings";
import { messageOf, unwrap } from "@/lib/ipc";
import { runBusyAction } from "@/lib/use-busy-action";

/// Creates the read-only support bundle for the given game, or for the app when no game is selected.
export function useSupportBundle(gameId: string | null) {
	const [path, setPath] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const [failure, setFailure] = useState<string | null>(null);

	const create = async () => {
		await runBusyAction(
			async () => {
				setPath(await unwrap(commands.createSupportBundle(gameId)));
			},
			setBusy,
			(error) => setFailure(messageOf(error)),
			() => setFailure(null),
		);
	};

	return { path, busy, failure, setFailure, create };
}

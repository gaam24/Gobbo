import { useEffect, useState } from "react";
import { commands, type IntegrityStatus, type ModDetails as ModDetailsData, type Status } from "@/bindings";
import { messageOf, unwrap } from "@/lib/ipc";

/// Reads the full details of the selected mod and owns the integrity check.
export function useModDetails({
	gameId,
	folder,
	status,
	onError,
}: {
	gameId: string;
	folder: string;
	status: Status;
	onError: (message: string) => void;
}) {
	const [details, setDetails] = useState<ModDetailsData | null>(null);
	// Keyed by folder so a check that finishes after a selection change never shows another mod's result.
	const [integrity, setIntegrity] = useState<{
		folder: string;
		status: IntegrityStatus;
	} | null>(null);
	const [verifying, setVerifying] = useState(false);

	useEffect(() => {
		let active = true;
		void (async () => {
			try {
				const loadedDetails = await unwrap(commands.modDetails(gameId, folder));
				// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`, which flow analysis cannot see.
				if (active) setDetails(loadedDetails);
			} catch (failure) {
				// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`, which flow analysis cannot see.
				if (active) onError(messageOf(failure));
			}
		})();
		return () => {
			active = false;
		};
		// The list reloads after every change, so the files stay in step with the profile.
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- `status` reloads the details on purpose.
	}, [gameId, folder, status, onError]);

	const verifyFiles = () => {
		if (verifying) return;
		setVerifying(true);
		void (async () => {
			try {
				const result = await unwrap(commands.verifyIntegrity(gameId, folder));
				setIntegrity({ folder, status: result });
			} catch (failure) {
				onError(messageOf(failure));
			}

			setVerifying(false);
		})();
	};

	return { details, integrity, verifying, verifyFiles };
}

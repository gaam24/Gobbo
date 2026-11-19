import { useEffect, useState } from "react";

import { commands } from "@/bindings";

/// The platform never changes while the app runs, so every caller shares one request.
let platform: Promise<string | null> | null = null;

async function askPlatform(): Promise<string | null> {
	try {
		return await commands.platform();
	} catch {
		// No backend: the interface keeps the platform-neutral layout.
		return null;
	}
}

/// The operating system the backend runs on (`windows`, `linux` or `macos`), or `null` while it is not known.
/// Without the backend, for example in a plain browser preview, it stays `null`.
export function usePlatform(): string | null {
	const [value, setValue] = useState<string | null>(null);

	useEffect(() => {
		let active = true;
		void (async () => {
			const pending = platform ?? askPlatform();
			platform = pending;
			const loaded = await pending;
			// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`, which flow analysis cannot see.
			if (active) setValue(loaded);
		})();
		return () => {
			active = false;
		};
	}, []);

	return value;
}

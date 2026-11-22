import { useState } from "react";
import { writeClipboard } from "@/lib/open";

const silentReport = () => {
	// The lines stay on screen for a manual copy.
};

/// Copies formatted lines to the clipboard. A failure is silent: the secure context may be missing, and the lines stay
/// on screen for a manual copy. `onCopied` runs only after a successful write, for views that track which lines were
/// copied rather than just whether the button was pressed.
export function useCopyLines() {
	const [copied, setCopied] = useState(false);
	const copyLines = async (lines: string[], onCopied?: () => void) => {
		if (await writeClipboard(lines.join("\n"), silentReport)) {
			setCopied(true);
			onCopied?.();
		}
	};
	const resetCopied = () => setCopied(false);
	return { copied, copyLines, resetCopied };
}

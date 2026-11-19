import type { ReactNode } from "react";
import { usePanelRef } from "react-resizable-panels";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { useTranslation } from "@/i18n";

/// The vertical split between the mod list and the console. This module is loaded on demand,
/// so `react-resizable-panels` stays out of the first-screen bundle until the console opens.
export function ConsoleSplit({
	main,
	console,
	defaultShare,
	resetShare,
	onShare,
}: {
	/// The mod list panel. It keeps its height when the console is closed.
	main: ReactNode;
	/// The console panel content.
	console: ReactNode;
	/// Starting share of the console, in percent. Remembered by the caller between mounts.
	defaultShare: number;
	/// The share a double click on the handle brings back.
	resetShare: number;
	onShare: (share: number) => void;
}) {
	const { t } = useTranslation();
	const consolePanel = usePanelRef();

	return (
		<ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
			<ResizablePanel id="mods" minSize={160}>
				{main}
			</ResizablePanel>
			<ResizableHandle
				withHandle
				aria-label={t("console.resize")}
				title={t("console.resizeHint")}
				onDoubleClick={() => consolePanel.current?.resize(`${resetShare}%`)}
			/>
			<ResizablePanel
				id="console"
				panelRef={consolePanel}
				defaultSize={`${defaultShare}%`}
				minSize={120}
				maxSize="80%"
				onResize={(size) => onShare(size.asPercentage)}
			>
				{console}
			</ResizablePanel>
		</ResizablePanelGroup>
	);
}

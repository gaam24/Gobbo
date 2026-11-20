import { Component, type ErrorInfo, type ReactNode } from "react";

import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";

interface ErrorBoundaryProps {
	children: ReactNode;
}

interface ErrorBoundaryState {
	error: Error | null;
}

/// Keeps a render failure inside one screen. React must use a class for this; the fallback is a function component so it
/// can use the same translations as the rest of the app. The data lives in the Rust backend, so a reload never loses a
/// profile or a mod.
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
	override state: ErrorBoundaryState = { error: null };

	static getDerivedStateFromError(error: Error): ErrorBoundaryState {
		return { error };
	}

	override componentDidCatch(error: Error, info: ErrorInfo) {
		// The backend log survives the window and is the file a support bundle carries.
		void (async () => {
			try {
				await commands.logFrontendError(error.message, info.componentStack ?? "");
			} catch {
				// Logging is best-effort: a failed log must not mask the render error this boundary is handling.
			}
		})();
	}

	reset = () => this.setState({ error: null });

	override render() {
		if (this.state.error) return <ErrorFallback error={this.state.error} onReset={this.reset} />;
		return this.props.children;
	}
}

function ErrorFallback({ error, onReset }: { error: Error; onReset: () => void }) {
	const { t } = useTranslation();
	return (
		<div role="alert" className="mx-auto grid max-w-md gap-4 p-10">
			<div className="grid gap-1.5">
				<h1 className="text-lg font-semibold">{t("errorBoundary.title")}</h1>
				<p className="text-muted-foreground text-sm">{t("errorBoundary.body")}</p>
			</div>
			<details className="text-muted-foreground text-xs">
				<summary className="cursor-pointer select-none">{t("common.details")}</summary>
				<pre className="mt-2 overflow-x-auto rounded-md border p-2 font-mono whitespace-pre-wrap">{error.message}</pre>
			</details>
			<div className="flex flex-wrap gap-2">
				<Button onClick={() => window.location.reload()}>{t("errorBoundary.reload")}</Button>
				<Button variant="outline" onClick={onReset}>
					{t("common.retry")}
				</Button>
			</div>
		</div>
	);
}

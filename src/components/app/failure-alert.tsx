import type { ReactNode } from "react";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";

/// Something that failed to load, what went wrong, and the retry next to it. Every failed read on a page uses this one
/// block, so a failure always has the same shape and its way out is always in the same place.
export function FailureAlert({
	title,
	children,
	onRetry,
	retrying = false,
}: {
	title: ReactNode;
	/// The error text, or a list of them.
	children?: ReactNode;
	onRetry?: () => void;
	/// The retry is running, so its button waits.
	retrying?: boolean;
}) {
	const { t } = useTranslation();
	return (
		<Alert variant="destructive">
			<AlertTitle>{title}</AlertTitle>
			{children && <AlertDescription>{children}</AlertDescription>}
			{onRetry && (
				<AlertAction>
					<Button variant="outline" size="sm" onClick={onRetry} disabled={retrying}>
						{t("common.retry")}
					</Button>
				</AlertAction>
			)}
		</Alert>
	);
}

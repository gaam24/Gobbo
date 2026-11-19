import { ErrorRecovery } from "@/components/app/error-recovery";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { messageOf } from "@/lib/ipc";

/// The failed status of the game screen: the message, the recovery action the error names, and a retry. The mod list and
/// the game settings show the same block, so the two pages cannot drift.
export function StatusError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
	const { t } = useTranslation();
	return (
		<div className="mx-auto flex max-w-md flex-col items-center gap-4 p-10 text-center">
			<FieldError>{messageOf(error)}</FieldError>
			<div className="flex flex-wrap items-center justify-center gap-2">
				<ErrorRecovery error={error} onRetry={onRetry} />
				<Button onClick={onRetry}>{t("common.retry")}</Button>
			</div>
		</div>
	);
}

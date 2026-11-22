import { TextCell } from "@/components/app/text-cell";
import { useTranslation } from "@/i18n";

/// The statuses a provider gives a page, each with its words in the interface. Any other status is shown as the provider
/// names it.
const KNOWN_STATUSES = ["published", "not_published", "hidden", "under_moderation", "removed", "removed_by_staff"] as const;

/// The words of a page status in the interface language. A status not listed in `KNOWN_STATUSES` stays as the
/// provider names it.
export function usePageStatus(): (status: string) => string {
	const { t } = useTranslation();
	return (status) => {
		const known = KNOWN_STATUSES.find((name) => name === status);
		return known ? t(`mods.sourceStatus.${known}`) : status;
	};
}

/// The status of a provider page, in the interface language.
export function PageStatusCell({ status }: { status: string | null | undefined }) {
	const statusWord = usePageStatus();
	return <TextCell value={status ? statusWord(status) : null} />;
}

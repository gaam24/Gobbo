import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { useTranslation } from "@/i18n";

/// The list has no mod to show for the search and filter: says so and offers to clear them.
export function NoMatch({ onClearFilters }: { onClearFilters: () => void }) {
	const { t } = useTranslation();
	return (
		<Empty className="px-6 py-10">
			<EmptyHeader>
				<EmptyTitle>{t("mods.noMatch")}</EmptyTitle>
			</EmptyHeader>
			<EmptyContent>
				<Button variant="outline" size="sm" onClick={onClearFilters}>
					{t("mods.clearFilters")}
				</Button>
			</EmptyContent>
		</Empty>
	);
}

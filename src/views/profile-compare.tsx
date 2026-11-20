import { useState } from "react";
import { type CompareRow, type CompareSide, commands, type ProfileView } from "@/bindings";
import { BoxList, BoxListRow, SectionHeading } from "@/components/app/content-blocks";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// Compares the active profile with another profile of the game: the mods only one of them lists, and the mods
/// both list with another version or state.
export function ProfileCompareDialog({
	gameId,
	profiles,
	active,
	open,
	onClose,
}: {
	gameId: string;
	profiles: ProfileView[];
	/// The profile in use. The other one is chosen in the dialog.
	active: ProfileView;
	open: boolean;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const others = profiles.filter((profile) => profile.id !== active.id);
	const [chosen, setChosen] = useState<string | null>(null);
	const other = others.find((profile) => profile.id === chosen) ?? others[0] ?? null;
	const otherId = other?.id ?? null;
	/// The pair being compared. An answer for another pair is ignored, so a new choice shows "comparing" until its own
	/// answer arrives.
	const pair = open && otherId !== null ? `${active.id}:${otherId}` : null;
	const answer = useLoad(pair, async () => {
		try {
			return {
				result: await unwrap(commands.compareProfiles(gameId, active.id, otherId ?? "")),
				failed: false,
			};
		} catch {
			return { result: null, failed: true };
		}
	});
	const result = answer?.result ?? null;
	const failed = answer?.failed ?? false;

	/// The state of one side in words: the version it selects and whether it is on, or that the profile lacks the mod.
	const sideText = (side: CompareSide | null | undefined) => {
		if (!side) return t("profiles.compare.absent");
		const state = side.enabled ? t("mods.state.enabled") : t("mods.state.disabled");
		return `${side.version ?? side.folder} · ${state}`;
	};

	const onlyFirst = result?.rows.filter((row) => !row.second) ?? [];
	const onlySecond = result?.rows.filter((row) => !row.first) ?? [];
	const changed = result?.rows.filter((row) => row.first && row.second) ?? [];

	return (
		<ModalDialog title={t("profiles.compare.title")} description={t("profiles.compare.hint")} width="lg" open={open} onClose={onClose}>
			{others.length > 1 && (
				<Field>
					<FieldLabel htmlFor="compare-profile">{t("profiles.compare.with")}</FieldLabel>
					<Select value={otherId ?? undefined} onValueChange={setChosen}>
						<SelectTrigger id="compare-profile" className="w-full">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{others.map((profile) => (
								<SelectItem key={profile.id} value={profile.id}>
									{profile.name}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</Field>
			)}

			{failed && <p className="text-destructive text-sm">{t("profiles.compare.failed")}</p>}
			{!(failed || result) && <p className="text-muted-foreground text-sm">{t("profiles.compare.loading")}</p>}
			{result && result.rows.length === 0 && <p className="text-sm">{t("profiles.compare.same")}</p>}
			{result && result.rows.length > 0 && (
				<>
					<p className="text-muted-foreground text-sm tabular-nums">
						{t("profiles.compare.summary", {
							same: result.same,
							different: result.rows.length,
						})}
					</p>
					{onlyFirst.length > 0 && (
						<CompareList
							title={t("profiles.compare.onlyIn", { name: result.first_name })}
							rows={onlyFirst}
							text={(row) => sideText(row.first)}
						/>
					)}
					{onlySecond.length > 0 && (
						<CompareList
							title={t("profiles.compare.onlyIn", { name: result.second_name })}
							rows={onlySecond}
							text={(row) => sideText(row.second)}
						/>
					)}
					{changed.length > 0 && (
						<CompareList
							title={t("profiles.compare.changed")}
							rows={changed}
							text={(row) => `${sideText(row.first)}  →  ${sideText(row.second)}`}
						/>
					)}
				</>
			)}
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

/// A group of differing mods: each with its name and what the two profiles say about it.
function CompareList({ title, rows, text }: { title: string; rows: CompareRow[]; text: (row: CompareRow) => string }) {
	return (
		<section className="grid gap-2">
			<SectionHeading title={title} count={rows.length} />
			<BoxList scroll>
				{rows.map((row) => (
					<BoxListRow key={row.id}>
						<ItemContent className="gap-0.5">
							<ItemTitle className="w-full font-normal">{row.name}</ItemTitle>
							<ItemDescription className="font-mono text-xs">{text(row)}</ItemDescription>
						</ItemContent>
					</BoxListRow>
				))}
			</BoxList>
		</section>
	);
}

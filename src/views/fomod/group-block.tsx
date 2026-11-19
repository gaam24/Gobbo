import { cn } from "cn";
import { Check } from "lucide-react";
import { useId } from "react";
import type { FomodGroupState, FomodGroupView } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { GROUP_HINT, isLocked, isRadio, OPTION_TAG } from "@/views/fomod/model";

/// One group: its name and rule on top, the options below, and an inline error when the rule is broken. The kind and the
/// picked flag of each option come from the backend answer, so the list shows what the import applies. The radio role
/// and the locked hint are presentation of those backend kinds; the answer to a click is decided by the backend.
export function GroupBlock({
	group,
	state,
	error,
	onPick,
	first,
}: {
	group: FomodGroupView;
	state: FomodGroupState | undefined;
	error: string | null;
	onPick: (optionName: string) => void;
	first: boolean;
}) {
	const { t } = useTranslation();
	const headingId = useId();
	const radio = isRadio(state?.kind ?? group.kind);
	const listed = group.options.flatMap((option, index) => {
		const answer = state?.options[index];
		const kind = answer?.kind ?? option.kind;
		return kind === "not_usable" ? [] : [{ option, kind, picked: answer?.picked ?? false }];
	});
	const locked = isLocked(listed.map((entry) => entry.kind));
	const hint = locked ? t("fomod.group.locked") : t(GROUP_HINT[group.kind]);

	return (
		<div className={cn("grid gap-2", !first && "border-t pt-5")}>
			<div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
				<h4 id={headingId} className="text-sm font-semibold">
					{group.name}
				</h4>
				<span className="text-subtle-foreground text-xs">{hint}</span>
			</div>
			{error && <FieldError>{error}</FieldError>}
			{/* The role changes with the group kind, so the linter cannot resolve it; aria-labelledby is global and
			    supported by both radiogroup and group. */}
			{/* biome-ignore lint/a11y/useAriaPropsSupportedByRole: dynamic role, valid for both values. */}
			<div role={radio ? "radiogroup" : "group"} aria-labelledby={headingId} className="grid gap-0.5">
				{listed.map(({ option, kind, picked }) => {
					const tag = OPTION_TAG[kind];
					// Required options are always installed, and a SelectAll group installs all of them: neither takes a click.
					const fixed = kind === "required" || group.kind === "select_all";
					return (
						// The role and aria-checked change with the group kind (AGENTS §7.2 prescribes exactly this pattern),
						// so the linter cannot resolve them statically.
						// biome-ignore lint/a11y/useAriaPropsSupportedByRole: aria-checked is valid for both radio and checkbox.
						<button
							key={option.name}
							type="button"
							role={radio ? "radio" : "checkbox"}
							aria-checked={picked}
							disabled={fixed}
							className="hover:bg-muted focus-visible:ring-ring flex min-h-11 w-full items-start gap-3 rounded-lg px-3 py-2 text-left outline-none focus-visible:ring-2 disabled:cursor-default disabled:hover:bg-transparent"
							onClick={() => onPick(option.name)}
						>
							<span
								aria-hidden
								className={cn(
									"mt-0.5 grid size-4 shrink-0 place-items-center border",
									radio ? "rounded-full" : "rounded-lg",
									picked ? "border-foreground bg-foreground text-background" : "border-subtle-foreground",
								)}
							>
								{picked && (radio ? <span className="bg-background size-1.5 rounded-full" /> : <Check className="size-3" />)}
							</span>
							<span className="grid min-w-0 gap-0.5">
								<span className="flex flex-wrap items-center gap-2 text-sm font-medium">
									{option.name}
									{tag && (
										<Badge variant="outline" className="px-1.5 text-xs">
											{t(tag)}
										</Badge>
									)}
								</span>
								{option.description && <span className="text-muted-foreground text-sm">{option.description}</span>}
							</span>
						</button>
					);
				})}
			</div>
		</div>
	);
}

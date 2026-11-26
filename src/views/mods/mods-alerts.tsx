import { cn } from "cn";
import { ChevronDown, Download, ToggleRight } from "lucide-react";
import { useState } from "react";
import type { Status } from "@/bindings";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { useTranslation } from "@/i18n";
import { translate } from "@/lib/ipc";
import { readPref, writePref } from "@/lib/ui-prefs";

/// Whether the warnings stay open. The list can be long, so the choice is kept between windows like the layout.
const WARNINGS_OPEN_PREF = "mods-warnings-open";

const isOpen = (value: unknown): value is boolean => typeof value === "boolean";

/// The problems that block deployment and the warnings that do not. Both are lists of backend messages, translated by
/// their stable codes; the same block appears in the deployment preview. The warnings fold away, so a long list does
/// not push the mod table down. A missing mod loader gets its install button right next to the message.
export function ModsAlerts({
	problems,
	warnings,
	onInstallLoader,
	onTurnOn,
}: {
	problems: Status["problems"];
	warnings: Status["warnings"];
	onInstallLoader?: () => void;
	/// Turns on a mod another mod needs. A warning that names a turned-off requirement gets this button next to it.
	onTurnOn?: (modId: string) => void;
}) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(() => readPref(WARNINGS_OPEN_PREF, true, isOpen));

	const changeOpen = (next: boolean) => {
		setOpen(next);
		writePref(WARNINGS_OPEN_PREF, next);
	};

	return (
		<>
			{problems.length > 0 && (
				<Alert
					id="mods-problems"
					tabIndex={-1}
					variant="destructive"
					className="focus-visible:ring-ring/50 mx-6 mt-4 w-auto min-w-0 outline-none focus-visible:ring-2"
				>
					<AlertTitle>{t("mods.problems.title")}</AlertTitle>
					<AlertDescription>
						<ul className="list-disc pl-4 wrap-break-word">
							{problems.map((problem) => (
								<li key={`${problem.code}${JSON.stringify(problem.params)}`}>
									{translate(problem)}
									{problem.code === "status.loader_missing" && onInstallLoader && (
										<Button type="button" size="sm" className="mt-2 flex w-fit" onClick={onInstallLoader}>
											<Download aria-hidden /> {t("header.installLoader", { name: problem.params.name ?? "" })}
										</Button>
									)}
								</li>
							))}
						</ul>
					</AlertDescription>
				</Alert>
			)}
			{warnings.length > 0 && (
				<Collapsible open={open} onOpenChange={changeOpen} className="mx-6 mt-4">
					<Alert className="w-auto min-w-0">
						<AlertTitle>
							<CollapsibleTrigger asChild>
								<Button variant="ghost" size="sm" className="-m-1 h-auto w-full min-w-0 justify-start gap-1.5 p-1 font-semibold">
									<ChevronDown className={cn("size-3.5 shrink-0 transition-transform", !open && "-rotate-90")} aria-hidden />
									<span className="truncate">{t("mods.relations.warningsTitle")}</span>
									<span className="text-subtle-foreground text-xs font-normal tabular-nums">{warnings.length}</span>
								</Button>
							</CollapsibleTrigger>
						</AlertTitle>
						<CollapsibleContent>
							<AlertDescription>
								<ul className="list-disc pl-4 wrap-break-word">
									{warnings.map((warning, index) => {
										const requirementId = warning.code === "relations.requires_disabled" ? warning.params.requirement_id : undefined;
										// Two mods can need the same one; the button sits only next to the first message that names it.
										const turnOnId =
											requirementId !== undefined && warnings.findIndex((other) => other.params.requirement_id === requirementId) === index
												? requirementId
												: null;
										return (
											<li key={`${warning.code}${JSON.stringify(warning.params)}`}>
												{translate(warning)}
												{turnOnId !== null && onTurnOn && (
													<Button
														type="button"
														variant="outline"
														size="sm"
														className="mt-1.5 mb-1 flex w-fit"
														onClick={() => onTurnOn(turnOnId)}
													>
														<ToggleRight aria-hidden /> {t("mods.relations.turnOn", { name: warning.params.requirement ?? "" })}
													</Button>
												)}
											</li>
										);
									})}
								</ul>
							</AlertDescription>
						</CollapsibleContent>
					</Alert>
				</Collapsible>
			)}
		</>
	);
}

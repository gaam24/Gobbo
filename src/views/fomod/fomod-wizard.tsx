import { cn } from "cn";
import { X } from "lucide-react";
import { useRef, useState } from "react";
import type { FomodState, FomodStepView, GroupProblem } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import type { FomodChoice } from "@/lib/fomod";
import { messageOf } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";
import { GroupBlock } from "@/views/fomod/group-block";
import { groupKey, selectionKey, stepState } from "@/views/fomod/model";
import { type FomodTarget, fomodFromChoices, readFomodState, toggleFomodPick } from "@/views/fomod/use-fomod";

/// FOMOD installer as a dialog: one visible step at a time on the left-hand list, its groups on the right. The steps
/// shown, the group rules, the option kinds and the defaults all come from the backend, and so does the answer to a
/// click (`fomod_toggle_pick`). The wizard only renders that answer and sends its selection back when confirmed.
///
/// The command calls live in this feature hook file and not in `src/lib/use-import.ts`: the wizard is a dialog whose
/// installer identity is owned by the caller, and there is no app-wide FOMOD state to hang a hook on. Passing the
/// identity as a prop keeps one command path for the import and the reinstall dialogs.
export function FomodWizard({
	moduleName,
	steps,
	fileName,
	game,
	target,
	initialChoices,
	onConfirm,
	onSkip,
}: {
	moduleName: string;
	steps: FomodStepView[];
	fileName: string;
	/// The game whose archive the installer belongs to, sent with each command.
	game: string;
	/// The installer's archive: a loose file for an import, an installed version's copy for a reinstall.
	target: FomodTarget;
	/// The answers a reinstall starts from. Omitted for a new import, which uses the installer's defaults.
	initialChoices?: FomodChoice[];
	onConfirm: (choices: FomodChoice[]) => void;
	onSkip: () => void;
}) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	/// The backend's answer to the last click. Until one happens, the initial answer (defaults or saved answers) shows.
	const [clicked, setClicked] = useState<FomodState | null>(null);
	/// The position in the visible steps, not in the XML: a step hidden by `<visible>` is never shown or visited.
	const [current, setCurrent] = useState(0);
	/// Stable keys for the step list: names may repeat, so a repeated one gets an ordinal. The list never reorders.
	const [stepKeys] = useState(() => {
		const seen = new Map<string, number>();
		return steps.map((item) => {
			const count = (seen.get(item.name) ?? 0) + 1;
			seen.set(item.name, count);
			return count === 1 ? item.name : `${item.name}#${count}`;
		});
	});
	/// Inline errors by group position, shown under the group they concern.
	const [errors, setErrors] = useState<Record<string, string>>({});
	const [confirmingSkip, setConfirmingSkip] = useState(false);
	/// Clicks can overlap; only the newest backend answer may replace what is on screen.
	const request = useRef(0);

	const onFailure = (error: unknown) => setFailure(messageOf(error));
	const loadKey = `${game}\n${target.kind}\n${target.kind === "archive" ? target.path : target.folder}`;
	const initial = useLoad<FomodState>(
		loadKey,
		async () => {
			const defaults = await readFomodState(game, target, null);

			if (!initialChoices || initialChoices.length === 0) return defaults;
			return fomodFromChoices(game, target, initialChoices);
		},
		{ onError: onFailure },
	);
	const answer = clicked ?? initial ?? null;

	const visible = answer?.visible ?? [];
	const position = Math.min(current, Math.max(visible.length - 1, 0));
	const stepIndex = visible[position];
	const step = stepIndex === undefined ? undefined : steps[stepIndex];
	const last = visible.length - 1;
	const changed = clicked !== null && initial !== undefined && selectionKey(clicked.selection) !== selectionKey(initial.selection);

	/// Asks the backend to apply one click. The group rule, the saved-choice rule and the required/unusable options are
	/// all decided there; the wizard just shows the answer it gets back.
	const pick = async (atStep: number, atGroup: number, optionName: string) => {
		if (!answer) return;
		const key = groupKey(atStep, atGroup);
		setErrors((prev) => {
			const next = { ...prev };
			delete next[key];
			return next;
		});
		const at = ++request.current;

		try {
			const next = await toggleFomodPick(game, target, answer.selection, atStep, atGroup, optionName);

			if (at !== request.current) return;
			setClicked(next);
			setFailure(null);
		} catch (error) {
			if (at === request.current) onFailure(error);
		}
	};

	const problemText = (problem: GroupProblem, group: string): string => {
		if (problem === "pick_one") return t("fomod.error.exactlyOne", { group });

		if (problem === "pick_at_least_one") return t("fomod.error.atLeastOne", { group });
		return t("fomod.error.atMostOne", { group });
	};

	/// Checks the groups of the given steps in one answer. Failing groups get their message; the result says whether all
	/// passed. With `focusFirst`, a failure in an earlier step moves the wizard there so the error is visible.
	const check = (value: FomodState, indices: number[], focusFirst = false): boolean => {
		const found: Record<string, string> = {};
		let firstFailing = -1;

		for (const index of indices) {
			const item = steps[index];

			if (!item) continue;

			for (const [groupIndex, group] of item.groups.entries()) {
				const problem = value.steps[index]?.groups[groupIndex]?.problem;

				if (problem) {
					found[groupKey(index, groupIndex)] = problemText(problem, group.name);

					if (firstFailing < 0) firstFailing = value.visible.indexOf(index);
				}
			}
		}

		setErrors(found);

		if (focusFirst && firstFailing >= 0) setCurrent(firstFailing);
		return Object.keys(found).length === 0;
	};

	const next = () => {
		if (stepIndex === undefined || !answer) return;

		if (check(answer, [stepIndex])) setCurrent(position + 1);
	};

	const install = () => {
		if (!answer) return;

		if (check(answer, answer.visible, true)) onConfirm(answer.selection);
	};

	/// Closing with choices made asks first. Without choices it closes at once.
	const requestClose = () => {
		if (changed) setConfirmingSkip(true);
		else onSkip();
	};

	return (
		<Dialog open onOpenChange={(open) => !open && requestClose()}>
			<DialogContent
				showCloseButton={false}
				onInteractOutside={(event) => {
					if (changed) event.preventDefault();
				}}
				className="bg-popover text-popover-foreground ring-foreground/10 flex max-h-[85vh] w-full max-w-[min(880px,calc(100%-2rem))] flex-col gap-0 overflow-hidden rounded-xl p-0 ring-1 sm:max-w-[min(880px,calc(100%-2rem))]"
			>
				<header className="flex items-start justify-between gap-4 border-b px-6 py-5">
					<div className="grid min-w-0 gap-1">
						<DialogTitle className="text-xl leading-tight font-semibold text-balance">{moduleName || t("fomod.fallbackTitle")}</DialogTitle>
						<DialogDescription className="text-muted-foreground text-sm">{t("fomod.description", { file: fileName })}</DialogDescription>
					</div>
					<Button variant="ghost" size="icon" aria-label={t("common.close")} onClick={requestClose}>
						<X />
					</Button>
				</header>

				<div className="grid min-h-0 flex-1 grid-cols-[200px_minmax(0,1fr)] overflow-hidden">
					<aside className="grid content-start gap-3 overflow-y-auto border-r px-4 py-5">
						<p className="text-subtle-foreground font-mono text-xs">
							{t("fomod.stepCounter", { current: position + 1, total: visible.length })}
						</p>
						<ol className="grid gap-1">
							{visible.map((index, at) => {
								const item = steps[index];

								if (!item) return null;
								const state = stepState(at, position);
								return (
									<li
										key={stepKeys[index] ?? item.name}
										aria-current={at === position ? "step" : undefined}
										className={cn("grid gap-0.5 rounded-lg px-3 py-2", at === position && "bg-muted")}
									>
										<span className="text-sm font-medium">{item.name}</span>
										<span className="text-subtle-foreground text-xs">{t(state)}</span>
									</li>
								);
							})}
						</ol>
					</aside>

					<div className="grid min-h-0 content-start gap-6 overflow-y-auto px-6 py-5">
						{failure && <FieldError>{t("fomod.readFailed", { detail: failure })}</FieldError>}
						{answer === null && failure === null && <p className="text-muted-foreground text-sm">{t("fomod.reading")}</p>}
						{answer && step && stepIndex !== undefined && (
							<section aria-labelledby="fomod-step-title" className="grid gap-4">
								<h3 id="fomod-step-title" className="text-base font-semibold">
									{step.name}
								</h3>
								{step.groups.map((group, groupIndex, all) => {
									const key = groupKey(stepIndex, groupIndex);
									return (
										<GroupBlock
											key={key}
											group={group}
											state={answer.steps[stepIndex]?.groups[groupIndex]}
											error={errors[key] ?? null}
											onPick={(optionName) => void pick(stepIndex, groupIndex, optionName)}
											first={groupIndex === 0 && all.length > 0}
										/>
									);
								})}
							</section>
						)}
					</div>
				</div>

				{confirmingSkip && (
					<div role="alert" className="bg-muted flex flex-wrap items-center justify-between gap-3 border-t px-6 py-3 text-sm">
						<span>{t("fomod.confirmSkip")}</span>
						<div className="flex gap-2">
							<Button variant="ghost" size="sm" onClick={() => setConfirmingSkip(false)}>
								{t("fomod.keepChoosing")}
							</Button>
							<Button variant="outline" size="sm" onClick={onSkip}>
								{t("fomod.skip")}
							</Button>
						</div>
					</div>
				)}

				<footer className="flex flex-wrap items-center justify-between gap-3 border-t px-6 py-4">
					<div className="flex items-center gap-2">
						<Button variant="ghost" onClick={requestClose}>
							{t("fomod.skip")}
						</Button>
					</div>
					<div className="flex flex-wrap items-center gap-3">
						<span className="text-muted-foreground text-sm tabular-nums">{t("fomod.selectedCount", { count: answer?.picked ?? 0 })}</span>
						<Button variant="outline" disabled={position === 0} onClick={() => setCurrent(position - 1)}>
							{t("fomod.back")}
						</Button>
						{position === last || visible.length === 0 ? (
							<Button disabled={!answer} onClick={install}>
								{t("fomod.install")}
							</Button>
						) : (
							<Button disabled={!answer} onClick={next}>
								{t("fomod.next")}
							</Button>
						)}
					</div>
				</footer>
			</DialogContent>
		</Dialog>
	);
}

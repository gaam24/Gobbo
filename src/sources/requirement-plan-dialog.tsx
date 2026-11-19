import { CircleAlert, CircleCheck, Download, ToggleRight } from "lucide-react";
import { useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands, type RequirementAction, type RequirementStep } from "@/bindings";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { translate, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

const ICON = {
	already_on: CircleCheck,
	enable: ToggleRight,
	download: Download,
	blocked: CircleAlert,
} as const satisfies Record<RequirementAction, typeof CircleCheck>;

const TONE = {
	already_on: "text-muted-foreground",
	enable: "text-foreground",
	download: "text-foreground",
	blocked: "text-destructive",
} as const satisfies Record<RequirementAction, string>;

/// The requirement closure before it runs: direct and transitive requirements, one chosen alternative per group, the
/// cycles and the reason a step cannot be installed. One confirm installs every step and saves the list once.
export function RequirementPlanDialog({ folder, onClose }: { folder: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { gameId, gameActions } = useWorkspace();
	const [busy, setBusy] = useState(false);
	const [failure, setFailure] = useState<string | null>(null);
	const plan = useLoad(`requirements:${folder}`, () => unwrap(commands.previewRequirements(gameId ?? "", folder)));
	const installable = (plan?.enable ?? 0) + (plan?.download ?? 0);

	const install = async () => {
		setBusy(true);
		setFailure(null);
		const ok = await gameActions.installRequirements(folder);
		setBusy(false);
		if (ok) onClose();
		else setFailure(t("requirements.installFailed"));
	};

	return (
		<ModalDialog title={t("requirements.title")} description={t("requirements.hint")} width="xl" onClose={onClose}>
			{!plan ? (
				<p className="text-muted-foreground text-sm">{t("requirements.loading")}</p>
			) : (
				<div className="grid gap-3">
					<p className="text-sm">
						{t("requirements.summary", {
							enable: plan.enable,
							download: plan.download,
							blocked: plan.blocked,
							root: plan.root,
						})}
					</p>
					{plan.steps.length === 0 ? (
						<p className="text-muted-foreground text-sm">{t("requirements.empty")}</p>
					) : (
						<ul className="grid gap-2">
							{plan.steps.map((step) => (
								<RequirementRow key={step.id} step={step} />
							))}
						</ul>
					)}
					{plan.cycles.length > 0 && (
						<ul className="border-destructive/40 bg-destructive/5 grid gap-1 rounded-lg border px-3 py-2 text-sm">
							{plan.cycles.map((cycle) => (
								<li key={`${cycle.code}:${cycle.params.path ?? ""}`} className="text-destructive">
									{translate(cycle)}
								</li>
							))}
						</ul>
					)}
					{failure && <FieldError>{failure}</FieldError>}
				</div>
			)}
			<DialogCloseFooter onClose={onClose} />
			{plan && plan.steps.length > 0 && (
				<div className="flex justify-end">
					<Button disabled={busy || installable === 0} onClick={() => void install()}>
						<Download aria-hidden />
						{busy ? t("requirements.installing") : t("requirements.install", { count: installable })}
					</Button>
				</div>
			)}
		</ModalDialog>
	);
}

function RequirementRow({ step }: { step: RequirementStep }) {
	const { t } = useTranslation();
	const Icon = ICON[step.action];
	return (
		<li className="flex items-start gap-2 rounded-lg border px-3 py-2.5">
			<Icon className={`mt-0.5 size-4 shrink-0 ${TONE[step.action]}`} aria-hidden />
			<div className="grid min-w-0 flex-1 gap-0.5">
				<div className="flex flex-wrap items-center gap-2">
					<span className="text-sm">{step.name}</span>
					<span className="text-muted-foreground text-xs">{t(`requirements.action.${step.action}`)}</span>
					{step.group && <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-xs">{t("requirements.oneOf")}</span>}
				</div>
				{step.message && <p className="text-destructive text-xs">{translate(step.message)}</p>}
				{step.source && (
					<p className="text-subtle-foreground font-mono text-xs">
						{step.source.provider}
						{" #"}
						{step.source.mod_id}
					</p>
				)}
			</div>
		</li>
	);
}

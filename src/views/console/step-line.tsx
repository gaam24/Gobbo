import type { StepReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import { errorText, translate } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";

/// One step of the last launch: its phase and state, what it wrote, and the end of its log.
export function StepLine({ step }: { step: StepReport }) {
	const { t, i18n } = useTranslation();
	const message =
		step.status === "failed"
			? errorText({ code: step.code, params: step.params, kind: "permanent" })
			: translate({ code: step.code, params: step.params });

	return (
		<li className="grid gap-1">
			<div className="flex items-baseline justify-between gap-3">
				<span className="font-medium">{step.id}</span>
				<span className="text-muted-foreground flex shrink-0 gap-2 text-xs">
					<span>{t(`runs.phase.${step.phase}`)}</span>
					<span>{t(`runs.status.${step.status}`)}</span>
				</span>
			</div>
			<p className="text-muted-foreground text-xs">{message}</p>
			{step.outputs.map((output) => (
				<p key={output.path} className="flex flex-wrap gap-2 font-mono text-xs break-all">
					<span>{output.path}</span>
					<span className="text-muted-foreground">{formatSize(Math.round(output.bytes / 1024), i18n.language)}</span>
				</p>
			))}
			{step.log_tail.length > 0 && (
				<pre className="bg-muted max-h-40 overflow-auto rounded-lg border p-2 font-mono text-xs break-all whitespace-pre-wrap">
					{step.log_tail.join("\n")}
				</pre>
			)}
		</li>
	);
}

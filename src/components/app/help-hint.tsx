import { CircleHelp } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";

/// A small question mark next to a label. The explanation lives in a tooltip, so a screen can stay short and a
/// first-time user can still find it. The trigger is a real button, so keyboard and screen-reader users get it too.
export function HelpHint({ children, label }: { children: ReactNode; label?: string }) {
	const { t } = useTranslation();
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					aria-label={label ?? t("common.help")}
					className="text-subtle-foreground hover:text-foreground -ml-1 size-5 shrink-0"
				>
					<CircleHelp className="size-4" aria-hidden />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top" align="start" className="max-w-80 text-sm leading-relaxed">
				{children}
			</TooltipContent>
		</Tooltip>
	);
}

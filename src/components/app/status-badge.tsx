import { cn } from "cn";
import type { ReactNode } from "react";
import { Badge } from "@/components/ui/badge";

export type StatusTone = "neutral" | "info" | "success" | "warning" | "danger" | "update";

const TONE = {
	neutral: "border-border text-muted-foreground",
	info: "border-win/40 text-win",
	success: "border-applied/40 text-applied",
	warning: "border-draft/40 text-draft",
	danger: "border-destructive/40 text-destructive",
	update: "border-update/40 text-update",
} as const satisfies Record<StatusTone, string>;

const DOT = {
	neutral: "bg-muted-foreground",
	info: "bg-win",
	success: "bg-applied",
	warning: "bg-draft",
	danger: "bg-destructive",
	update: "bg-update",
} as const satisfies Record<StatusTone, string>;

/// A status pill shared by games, profiles and mods. The text always carries the meaning; the dot and colour are
/// a second signal, never the only one.
export function StatusBadge({
	tone = "neutral",
	dot = false,
	children,
	className,
}: {
	tone?: StatusTone;
	dot?: boolean;
	children: ReactNode;
	className?: string;
}) {
	return (
		<Badge variant="outline" className={cn("gap-1.5 font-medium", TONE[tone], className)}>
			{dot && <span aria-hidden className={cn("size-2 shrink-0 rounded-full", DOT[tone])} />}
			{children}
		</Badge>
	);
}

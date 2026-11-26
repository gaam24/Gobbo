import { ArchiveRestore, Files, ListOrdered, Stethoscope, Wrench } from "lucide-react";
import { useUiActions } from "@/app/contexts/ui-context";
import type { Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTranslation } from "@/i18n";

/// The game's tools: the shared files, the load order, the snapshots and the two checks. They are kept out of the play
/// button's menu, which only starts the game, so a tool is never one wrong click away from launching it.
export function GameToolsMenu({ status }: { status: Status }) {
	const { t } = useTranslation();
	const { openDialog } = useUiActions();
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" aria-label={t("header.menu.tools")} title={t("header.menu.tools")}>
					<Wrench /> <span className="hidden @5xl:inline">{t("header.menu.tools")}</span>
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="w-64">
				<DropdownMenuItem onSelect={() => openDialog({ kind: "conflicts" })}>
					<Files /> {t("conflicts.title")}
					{status.conflict_count > 0 && <span className="text-muted-foreground ml-auto text-xs tabular-nums">{status.conflict_count}</span>}
				</DropdownMenuItem>
				{status.load_order && (
					<DropdownMenuItem onSelect={() => openDialog({ kind: "load-order" })}>
						<ListOrdered /> {t("loadOrder.title")}
					</DropdownMenuItem>
				)}
				<DropdownMenuItem onSelect={() => openDialog({ kind: "snapshots" })}>
					<ArchiveRestore /> {t("snapshots.title")}
				</DropdownMenuItem>
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={() => openDialog({ kind: "health" })}>
					<Stethoscope /> {t("health.title")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => openDialog({ kind: "setup" })}>
					<Wrench /> {t("setupCheck.title")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

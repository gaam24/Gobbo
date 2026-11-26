import { cn } from "cn";
import { EyeOff, type LucideIcon, Minimize2, Power } from "lucide-react";
import type { CloseAction } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useTranslation } from "@/i18n";

/// What the backend says about this close: whether the tray icon exists and whether a game would be stopped.
export interface CloseRequest {
	tray: boolean;
	game_running: boolean;
}

/// The icon colour carries the outcome: green is the choice Gobbo recommends, red is the one that ends the session.
/// The text still says what each choice does, so colour never works alone.
function CloseOption({
	icon: Icon,
	title,
	detail,
	tone,
	onClick,
}: {
	icon: LucideIcon;
	title: string;
	detail: string;
	tone?: "primary" | "destructive";
	onClick: () => void;
}) {
	return (
		<Button variant="outline" className="h-auto justify-start gap-3 px-3 py-2.5 text-left whitespace-normal" onClick={onClick}>
			<Icon
				className={cn("text-muted-foreground", tone === "primary" && "text-primary", tone === "destructive" && "text-destructive")}
				aria-hidden
			/>
			<span className="grid gap-0.5">
				<span>{title}</span>
				<span className="text-muted-foreground text-xs font-normal">{detail}</span>
			</span>
		</Button>
	);
}

/// The body of the closing dialog. Kept apart from the Tauri listener so its choices can be checked on their
/// own. The choice that keeps Gobbo working comes first, so the focus and Enter never end the app by accident.
export function CloseDialogChoice({
	request,
	remember,
	onRemember,
	onChoose,
}: {
	request: CloseRequest;
	remember: boolean;
	onRemember: (checked: boolean) => void;
	onChoose: (action: CloseAction) => void;
}) {
	const { t } = useTranslation();
	return (
		<>
			<div className="grid gap-2">
				{request.tray && (
					<CloseOption
						icon={EyeOff}
						title={t("closeDialog.hideToTray")}
						detail={t("closeDialog.hideDetail")}
						tone="primary"
						onClick={() => onChoose("hide_to_tray")}
					/>
				)}
				<CloseOption
					icon={Minimize2}
					title={t("closeDialog.minimize")}
					detail={t("closeDialog.minimizeDetail")}
					onClick={() => onChoose("minimize")}
				/>
				<CloseOption
					icon={Power}
					title={t("closeDialog.exit")}
					detail={request.game_running ? t("closeDialog.exitGameDetail") : t("closeDialog.exitDetail")}
					tone="destructive"
					onClick={() => onChoose("exit")}
				/>
			</div>
			<label htmlFor="close-remember" className="flex items-start gap-2">
				<Checkbox id="close-remember" className="mt-0.5" checked={remember} onCheckedChange={(checked) => onRemember(checked === true)} />
				<span className="grid gap-0.5">
					<span>{t("closeDialog.remember")}</span>
					<span className="text-muted-foreground text-xs">{t("closeDialog.rememberHint")}</span>
				</span>
			</label>
		</>
	);
}

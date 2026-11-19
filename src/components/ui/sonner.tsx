import { CircleCheckIcon, InfoIcon, OctagonXIcon, TriangleAlertIcon } from "lucide-react";
import type { CSSProperties } from "react";
import { Toaster as Sonner, type ToasterProps } from "sonner";

/// Toasts are short-lived, one-line results. Ongoing work and failures that offer recovery belong to the Working
/// panel, which is pinned bottom-right (`activity-center.tsx`). The toaster therefore pins to the top with an explicit
/// offset and a deliberate z-index above that panel, so the two surfaces can never occupy the same corner. A call site
/// must choose its position; a `bottom-right` wrapper default would silently collide with the Working panel.
type ToasterPosition = NonNullable<ToasterProps["position"]>;

interface ToasterOwnProps extends Omit<ToasterProps, "position"> {
	position: ToasterPosition;
	closeButtonAriaLabel?: string;
}

/// The app has one dark theme, so the toaster does not read the system theme.
function Toaster({ closeButtonAriaLabel, position, ...props }: ToasterOwnProps) {
	return (
		<Sonner
			theme="dark"
			position={position}
			offset={16}
			closeButton
			className="toaster group"
			icons={{
				success: <CircleCheckIcon className="size-4" />,
				info: <InfoIcon className="size-4" />,
				warning: <TriangleAlertIcon className="size-4" />,
				error: <OctagonXIcon className="size-4" />,
			}}
			style={
				{
					// Sonner paints each toast from these variables. Map every severity to the app's tokens so a toast
					// matches the Working panel: bg-card, text-card-foreground, border-border and rounded-xl.
					zIndex: 100,
					"--normal-bg": "var(--card)",
					"--normal-text": "var(--card-foreground)",
					"--normal-border": "var(--border)",
					"--info-bg": "var(--card)",
					"--info-text": "var(--card-foreground)",
					"--info-border": "var(--border)",
					"--success-bg": "var(--card)",
					"--success-text": "var(--applied)",
					"--success-border": "var(--border)",
					"--warning-bg": "var(--card)",
					"--warning-text": "var(--draft)",
					"--warning-border": "var(--border)",
					"--error-bg": "var(--card)",
					"--error-text": "var(--destructive)",
					"--error-border": "var(--border)",
					"--border-radius": "var(--radius-xl)",
				} as CSSProperties
			}
			toastOptions={{
				classNames: {
					toast: "bg-card text-card-foreground border-border rounded-xl text-sm shadow-lg",
					description: "text-muted-foreground",
					closeButton: "bg-card text-card-foreground border-border",
				},
				// Inline, so it wins over Sonner's own unlayered box-shadow rule.
				style: { boxShadow: "var(--shadow-lg)" },
				closeButtonAriaLabel,
			}}
			{...props}
		/>
	);
}

export { Toaster };

import { createContext, type ReactNode, useCallback, useContext, useMemo, useRef, useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { useTranslation } from "@/i18n";

interface Unsaved {
	/// Called by a form that keeps a draft. The provider only remembers whether one is dirty.
	setDirty: (dirty: boolean) => void;
	/// Runs `action` now, or asks first when the current form has unsaved changes. The action only runs after the user
	/// chooses to discard them, so every exit (rail, settings tabs, palette, Back) shares one guard.
	requestLeave: (action: () => void) => void;
}

const UnsavedContext = createContext<Unsaved | null>(null);

export function useUnsavedChanges(): Unsaved {
	const context = useContext(UnsavedContext);
	if (!context) {
		throw new Error("useUnsavedChanges must be used inside <UnsavedProvider>.");
	}
	return context;
}

/// One guard for the whole shell. The form owns the draft; this provider owns the question and the pending action.
export function UnsavedProvider({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const [pending, setPending] = useState<(() => void) | null>(null);
	/// The draft flag is read by event handlers only, so it is a ref: changing it must not re-render the whole shell on
	/// every keystroke in a settings form.
	const dirty = useRef(false);

	const setDirty = useCallback((value: boolean) => {
		dirty.current = value;
	}, []);

	const requestLeave = useCallback((action: () => void) => {
		if (dirty.current) setPending(() => action);
		else action();
	}, []);

	const confirm = useCallback(() => {
		const action = pending;
		setPending(null);
		// The form is no longer dirty before the action runs. Without this, navigating from the confirm handler asks
		// again: the navigation's own `requestLeave` would still see the draft and replace the action it is running with
		// another question. The dialog's close also clears `pending` after this handler, so a second question would vanish.
		dirty.current = false;
		action?.();
	}, [pending]);

	const value = useMemo((): Unsaved => ({ setDirty, requestLeave }), [requestLeave, setDirty]);

	return (
		<UnsavedContext.Provider value={value}>
			{children}
			<ConfirmDialog
				open={pending !== null}
				onOpenChange={(open) => {
					if (!open) setPending(null);
				}}
				title={t("settings.discardBack.title")}
				description={t("settings.discardBack.body")}
				confirmLabel={t("settings.discardBack.confirm")}
				onConfirm={confirm}
			/>
		</UnsavedContext.Provider>
	);
}

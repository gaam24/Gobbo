import { toast as sonner } from "sonner";

import type { NoticeAction } from "@/lib/notices";

/// One shape for a button that travels with a result, in a toast or in the durable notification list.
export type ToastAction = NoticeAction;

/// How a one-line result is styled. There is no `loading` severity: ongoing work lives in the Working panel only.
export type ToastSeverity = "info" | "success" | "error" | "warning";

interface ToastOptions {
	type?: ToastSeverity;
	title: string;
	description?: string;
	/// A button in the toast. The plain notice list does not keep it; `notifyDurable` is the path that does.
	action?: ToastAction;
	/// Milliseconds before the toast disappears. Zero or omitted keeps it until dismissed.
	timeout?: number;
	/// Reuse an existing toast's id to update it in place instead of stacking another card (see the toast dedupe).
	id?: string | number;
}

/// The app's small toast facade over Sonner. It keeps the call sites independent of the toast library and maps
/// the old `timeout` contract (zero means persistent) to Sonner's duration.
export const toast = {
	add({ id, type = "info", title, description, action, timeout }: ToastOptions): string | number {
		const options = {
			id,
			description,
			action,
			duration: timeout && timeout > 0 ? timeout : Number.POSITIVE_INFINITY,
		};
		if (type === "success") return sonner.success(title, options);
		if (type === "error") return sonner.error(title, options);
		if (type === "warning") return sonner.warning(title, options);
		return sonner.info(title, options);
	},
};

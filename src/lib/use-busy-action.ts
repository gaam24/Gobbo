/// Runs `action` with a busy flag cleared on both the success and the failure path. The failure is reported through
/// `onError` and does not escape; a caller that needs to rethrow does its own catch. No `finally`: the React Compiler
/// skips the whole function when it contains one.
export async function runBusyAction(
	action: () => Promise<void>,
	setBusy: (busy: boolean) => void,
	onError: (error: unknown) => void,
	clearError?: () => void,
): Promise<void> {
	setBusy(true);

	if (clearError) clearError();

	try {
		await action();
	} catch (error) {
		onError(error);
	}

	setBusy(false);
}

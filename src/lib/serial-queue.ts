/// Runs async actions one after another. An action starts once the previous one has settled. A failed action does not
/// stop the next one: its rejection goes to the caller through the promise `run` returns, and the queue keeps going.
export function createQueue() {
	let tail: Promise<void> = Promise.resolve();
	const run = (action: () => Promise<unknown>): Promise<void> => {
		const previous = tail;
		const next = (async () => {
			await previous;
			await action();
		})();
		tail = (async () => {
			try {
				await next;
			} catch {
				// The caller sees the rejection through `next`; the queue itself keeps going.
			}
		})();
		return next;
	};
	/// Settles once every action queued so far has settled.
	const idle = (): Promise<void> => tail;
	return { run, idle };
}

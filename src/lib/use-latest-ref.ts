import { type RefObject, useEffect, useRef } from "react";

/// A stable ref that always holds the newest render's value. A callback registered once (a Tauri listener, a task
/// action) reads it when it runs instead of keeping the render that registered it. The ref is updated in an effect, so
/// reads before an effect runs still see the previous value, like a plain `useRef` mirror.
export function useLatestRef<T>(value: T): RefObject<T> {
	const ref = useRef(value);
	useEffect(() => {
		ref.current = value;
	});
	return ref;
}

/// Makes an exhaustive `switch` fail at compile time when a new union member is added.
export function assertNever(value: never): never {
	throw new Error(`Unhandled case: ${String(value)}`);
}

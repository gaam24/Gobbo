/// The first name that `label` gives for a number from 1 on and that no existing name uses. Names compare without
/// case and surrounding spaces, the same way the profile names are checked when they are saved.
export function freeName(label: (number: number) => string, taken: readonly string[]): string {
	const used = new Set(taken.map((name) => name.trim().toLocaleLowerCase()));

	for (let number = 1; ; number += 1) {
		const candidate = label(number);

		if (!used.has(candidate.trim().toLocaleLowerCase())) return candidate;
	}
}

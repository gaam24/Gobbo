/// The requirements of a mod in lists: the ones of one group share a list and are alternatives, and each other
/// requirement is a list of its own. Groups are scoped to the provider. The order of the requirements is kept.
export function sourceLists<T extends { source: { provider: string }; group?: number | null }>(requirements: T[]): T[][] {
	const lists: T[][] = [];
	const listOfGroup = new Map<string, T[]>();
	for (const requirement of requirements) {
		if (requirement.group == null) {
			lists.push([requirement]);
			continue;
		}
		const key = `${requirement.source.provider}:${requirement.group}`;
		const known = listOfGroup.get(key);
		if (known) known.push(requirement);
		else {
			const list = [requirement];
			listOfGroup.set(key, list);
			lists.push(list);
		}
	}
	return lists;
}

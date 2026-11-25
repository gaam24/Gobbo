/// The rules of a mod's tags in the inspector. They are pure, so the field and its tests use the same ones.

/// The mod's tags with `name` added at the end, or `null` when there is nothing to add: the name is blank once trimmed,
/// or the mod has that tag already. The name is trimmed before it is compared and stored.
export function withTag(tags: readonly string[], name: string): string[] | null {
	const tag = name.trim();

	if (tag === "" || tags.includes(tag)) {
		return null;
	}

	return [...tags, tag];
}

/// The game's tags that the mod does not have yet, in their order. The list offers these whatever the field holds: the
/// text never filters it.
export function tagSuggestions(allTags: readonly string[], tags: readonly string[]): string[] {
	return allTags.filter((tag) => !tags.includes(tag));
}

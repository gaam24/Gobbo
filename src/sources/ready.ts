import type { SourceProviderInfo } from "@/bindings";

/// The providers Gobbo can ask now: the ones that need no key, and the ones whose key is saved. Every check of a source
/// asks this set for its own provider, so a game on a provider without keys never waits for another provider's key.
export function readyProviders(providers: SourceProviderInfo[], keysSaved: Record<string, boolean>): ReadonlySet<string> {
	return new Set(providers.filter((provider) => !provider.needs_key || keysSaved[provider.id] === true).map((provider) => provider.id));
}

/// Whether any of `ids` (a game's providers, or a mod's sources) can be asked now.
export function anyReady(ready: ReadonlySet<string>, ids: Iterable<string>): boolean {
	for (const id of ids) if (ready.has(id)) return true;
	return false;
}

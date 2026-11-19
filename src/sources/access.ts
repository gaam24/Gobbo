import type { SourceProviderInfo } from "@/bindings";

/// The providers the backend routes to the provider's page (`download_route` is `browser`): their updates and downloads
/// open the files page, and the rest download in Gobbo. The backend decides the route once; this only reads it.
export function browserProviders(providers: readonly SourceProviderInfo[]): ReadonlySet<string> {
	return new Set(providers.filter((provider) => provider.download_route === "browser").map((provider) => provider.id));
}

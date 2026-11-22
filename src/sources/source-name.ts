import { useSettingsData } from "@/app/contexts/settings-context";
import type { SourceProviderInfo } from "@/bindings";

/// The name of a source provider as the backend gives it, falling back to the id for a provider the build does not know.
export function sourceNameOf(providers: readonly SourceProviderInfo[], id: string): string {
	return providers.find((info) => info.id === id)?.name ?? id;
}

export function useSourceName(): (provider: string) => string {
	const { providers } = useSettingsData();
	return (provider) => sourceNameOf(providers, provider);
}

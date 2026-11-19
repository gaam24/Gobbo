import { useSettingsData } from "@/app/contexts/settings-context";

/// The name of a source provider as the backend gives it, falling back to the id for a provider the build does not know.
export function useSourceName(): (provider: string) => string {
	const { providers } = useSettingsData();
	return (provider) => providers.find((info) => info.id === provider)?.name ?? provider;
}

import type { SourceLinkStatus } from "@/bindings";

/// The providers whose download links the system sends to another program. The status list names only the schemes the
/// settings take, so a provider here is turned on but not handled by Gobbo.
export function lostLinkProviders(statuses: SourceLinkStatus[]): string[] {
	return [...new Set(statuses.filter((status) => !status.registered).map((status) => status.provider))];
}

/// The schemes another program handles, as the settings warn about them.
export function lostLinkSchemes(statuses: SourceLinkStatus[]): string[] {
	return [...new Set(statuses.filter((status) => !status.registered).map((status) => `${status.scheme}://`))];
}

import { ExternalLink, FolderOpen } from "lucide-react";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import type { Status } from "@/bindings";
import { ConfirmButton } from "@/components/confirm-button";
import { Button } from "@/components/ui/button";
import { Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { Section, SectionBlock } from "@/views/settings/controls";

/// The game's tools and their prerequisites. A tool without its program links to its Nexus page; a tool with missing
/// prerequisites can install them into the game's Proton prefix after a confirmation.
export function ToolsSection({
	status,
	toolState,
	onInstallPrerequisites,
	onOpen,
}: {
	status: Status;
	toolState: (folder: string | null | undefined) => string;
	onInstallPrerequisites: (toolId: string) => void | Promise<void>;
	onOpen: (path: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const { openSource } = useWorkspaceActions();

	if (status.tools.length === 0) return null;

	return (
		<Section id="settings-tools" title={t("settings.tools.title")} hint={t("settings.tools.hint")}>
			<SectionBlock>
				<ul className="grid gap-3">
					{status.tools.map((tool) => (
						<Item asChild key={tool.id} variant="outline" size="sm" className="min-w-0">
							<li>
								<ItemContent className="min-w-0 gap-0.5">
									<ItemTitle>{tool.name}</ItemTitle>
									<ItemDescription>
										{toolState(tool.folder)}
										{" · "}
										{t("settings.tools.output", { output: tool.output })}
										{tool.missing.length > 0 && ` · ${t("settings.tools.prerequisitesMissing", { names: tool.missing.join(", ") })}`}
									</ItemDescription>
								</ItemContent>
								<ItemActions className="flex-wrap">
									{!tool.folder && tool.source && (
										<Button
											type="button"
											variant="outline"
											size="sm"
											onClick={() => {
												if (tool.source) openSource(tool.source.provider, tool.source.mod_id);
											}}
										>
											<ExternalLink /> {t("settings.tools.getFromSource")}
										</Button>
									)}
									{tool.missing.length > 0 && (
										<ConfirmButton
											label={t("settings.tools.installPrerequisites")}
											title={t("settings.tools.installPrerequisitesTitle", {
												name: tool.name,
											})}
											description={t("settings.tools.installPrerequisitesDescription", { names: tool.missing.join(", ") })}
											confirmLabel={t("settings.tools.installPrerequisites")}
											destructive={false}
											onConfirm={() => onInstallPrerequisites(tool.id)}
										/>
									)}
									<Button
										type="button"
										variant="outline"
										size="sm"
										disabled={!tool.folder}
										onClick={() => tool.folder && void onOpen(tool.folder)}
									>
										<FolderOpen /> {t("settings.tools.open")}
									</Button>
								</ItemActions>
							</li>
						</Item>
					))}
				</ul>
			</SectionBlock>
		</Section>
	);
}

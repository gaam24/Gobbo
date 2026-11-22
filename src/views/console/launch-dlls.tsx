import type { DllReport } from "@/bindings";
import { BoxList, BoxListRow, Disclosure, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { useCopyLines } from "@/lib/use-copy-lines";

/// The DLLs an overlay launch found in the mod layer, and the static imports the launcher points at that layer. The
/// first list says what the game can load; the second says what was patched before the first instruction.
export function LaunchDlls({ dlls }: { dlls: DllReport }) {
	const { t } = useTranslation();
	const { copied, copyLines } = useCopyLines();
	const { layer, injected } = dlls;
	const copy = async () => {
		const lines = [...injected.map((dll) => `injected ${dll.name} ${dll.path} <- ${dll.source}`), ...layer.map((path) => `layer ${path}`)];
		await copyLines(lines);
	};

	return (
		<section aria-label={t("console.dlls")} className="grid gap-2">
			<SectionHeading
				title={t("console.dlls")}
				actions={
					(injected.length > 0 || layer.length > 0) && (
						<Button variant="ghost" size="sm" onClick={() => void copy()}>
							{copied ? t("console.dllsCopied") : t("console.dllsCopy")}
						</Button>
					)
				}
			/>
			<p className="text-muted-foreground text-xs">{t("console.dllsHint")}</p>

			<SectionHeading title={t("console.dllsInjected", { count: injected.length })} />
			{injected.length === 0 ? (
				<p className="text-muted-foreground text-xs">{t("console.dllsInjectedEmpty")}</p>
			) : (
				<BoxList scroll>
					{injected.map((dll) => (
						<BoxListRow key={`${dll.name}-${dll.path}`} className="flex-wrap gap-2 font-mono text-xs break-all">
							<span className="text-muted-foreground shrink-0">{dll.name}</span>
							<span>{dll.path}</span>
							<span className="text-subtle-foreground">{dll.source}</span>
						</BoxListRow>
					))}
				</BoxList>
			)}

			<SectionHeading title={t("console.dllsLayer", { count: layer.length })} />
			{layer.length === 0 ? (
				<p className="text-muted-foreground text-xs">{t("console.dllsLayerEmpty")}</p>
			) : (
				<Disclosure title={t("console.dllsLayerShow")} defaultOpen={layer.length <= 12}>
					<BoxList scroll tall>
						{layer.map((path) => (
							<BoxListRow key={path} className="font-mono text-xs break-all">
								{path}
							</BoxListRow>
						))}
					</BoxList>
				</Disclosure>
			)}
		</section>
	);
}

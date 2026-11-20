import { useCollections } from "@/app/contexts/collections-context";
import { CollectionInstallView } from "@/views/collections/collection-install-view";
import { CollectionLinkView } from "@/views/collections/collection-link-view";
import { CollectionPanelView } from "@/views/collections/collection-panel-view";
import { CollectionPreviewView } from "@/views/collections/collection-preview-view";
import { FomodWizard } from "@/views/fomod/fomod-wizard";

/// The one collection dialog. The preview, the install and the ongoing panel share the state in `CollectionsProvider`;
/// each screen renders its own dialog so its footer stays outside the scrolling body.
export function CollectionDialog({ onClose }: { onClose: () => void }) {
	const collections = useCollections();
	const close = () => onClose();

	if (collections.choice) {
		const choice = collections.choice;
		return (
			<FomodWizard
				moduleName={choice.module_name}
				steps={choice.steps}
				fileName={choice.name}
				game={choice.game}
				target={{ kind: "archive", path: choice.path }}
				onConfirm={(choices) => void collections.chooseFomod(choices)}
				onSkip={() => void collections.skip(false)}
			/>
		);
	}

	switch (collections.screen) {
		case "preview":
			return collections.preview ? <CollectionPreviewView preview={collections.preview} onClose={close} /> : null;
		case "install":
			return <CollectionInstallView onClose={close} />;
		case "panel":
			return <CollectionPanelView onClose={close} />;
		case "link":
			return <CollectionLinkView onClose={close} />;
		default:
			return <CollectionLinkView onClose={close} />;
	}
}

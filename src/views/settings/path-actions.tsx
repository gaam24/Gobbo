import { Copy, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { useTranslation } from "@/i18n";

/// The path and the two actions every path row needs.
export interface PathProps {
	path: string;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
}

/// The two buttons of a path: to open it in the file manager, and to copy it. Both wait for a path.
export function PathActions({ path, onOpen, onCopy }: PathProps) {
	const { t } = useTranslation();
	return (
		<ButtonGroup>
			<Button variant="outline" size="sm" disabled={!path} onClick={() => void onOpen(path)}>
				<FolderOpen /> {t("common.open")}
			</Button>
			<Button variant="outline" size="sm" disabled={!path} onClick={() => void onCopy(path)}>
				<Copy /> {t("common.copy")}
			</Button>
		</ButtonGroup>
	);
}

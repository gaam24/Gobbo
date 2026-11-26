import { useState } from "react";
import { StatusLine } from "@/components/app/content-blocks";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import { pickFile } from "@/lib/tauri";
import { EditorField } from "@/views/games/editor/fields";
import { FoundPicker } from "@/views/games/editor/found-picker";
import { relativeToFolder } from "@/views/games/editor/game-path";
import { QuickSection } from "@/views/games/editor/quick-section";
import type { QuickProps } from "@/views/games/editor/quick-types";

const PATH_SEPARATOR = /[\\/]/;
const EXE_SUFFIX = /\.exe$/i;

/// The name a program gets from its path: the file name without `.exe`.
function nameOf(path: string): string {
	return path.split(PATH_SEPARATOR).at(-1)?.replace(EXE_SUFFIX, "") ?? "";
}

/// The program Gobbo starts. The path field lists every program Gobbo found, whatever the field holds, and still takes a
/// path typed by hand. Browse picks a file inside the game folder.
export function QuickLaunch({ draft, onChange, inspection, folder }: QuickProps) {
	const { t } = useTranslation();
	const [outside, setOutside] = useState(false);
	const executables = draft.executables;
	const firstExecutable = executables[0];
	const found = inspection?.executables.map((executable) => executable.path) ?? [];

	const setExecutable = (path: string) => {
		setOutside(false);
		// A name that is not the name of the previous path was typed by the user, so it stays.
		const previousName = firstExecutable?.name ?? "";
		const previousPath = firstExecutable?.path ?? "";
		const typed = previousName !== "" && previousName !== nameOf(previousPath);
		onChange({
			...draft,
			executables: [{ name: typed ? previousName : nameOf(path), path }, ...executables.slice(1)],
		});
	};

	const browse = async () => {
		const picked = await pickFile();

		if (picked === null) return;

		const relative = relativeToFolder(folder, picked);

		if (relative === null) setOutside(true);
		else setExecutable(relative);
	};

	return (
		<QuickSection number={3} title={t("editor.quick.launchTitle")} hint={t("editor.quick.launchHint")}>
			<div className="grid gap-4 md:grid-cols-2">
				<EditorField id="editor-executable" label={t("editor.quick.executable")} hint={t("editor.quick.executableHint")}>
					<FoundPicker
						id="editor-executable"
						value={firstExecutable?.path ?? ""}
						onChange={setExecutable}
						found={found}
						menuLabel={t("editor.quick.foundPrograms")}
						placeholder="Game.exe"
						onBrowse={folder.trim() === "" ? undefined : () => void browse()}
					/>
					{outside && <StatusLine tone="error">{t("editor.quick.programOutside")}</StatusLine>}
				</EditorField>
				<EditorField id="editor-executable-name" label={t("editor.quick.executableName")}>
					<Input
						id="editor-executable-name"
						value={firstExecutable?.name ?? ""}
						placeholder={t("editor.quick.executableNamePlaceholder")}
						onChange={(event) => {
							const next = [...executables];
							next[0] = {
								...(next[0] ?? { name: "", path: "" }),
								name: event.currentTarget.value,
							};
							onChange({ ...draft, executables: next });
						}}
					/>
				</EditorField>
			</div>
			{executables.length > 1 && <p className="text-muted-foreground text-xs">{t("editor.quick.morePrograms")}</p>}
		</QuickSection>
	);
}

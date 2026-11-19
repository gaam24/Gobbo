import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import { EditorField } from "@/views/games/editor/fields";
import { QuickSection } from "@/views/games/editor/quick-section";
import type { QuickProps } from "@/views/games/editor/quick-types";

const PATH_SEPARATOR = /[\\/]/;
const EXE_SUFFIX = /\.exe$/i;

/// The program Gobbo starts. The field is a datalist, so a detected program is one click and a native Linux program
/// can still be typed by hand.
export function QuickLaunch({ draft, onChange, inspection }: QuickProps) {
	const { t } = useTranslation();
	const executables = draft.executables;
	const firstExecutable = executables[0];

	const setExecutable = (path: string) => {
		const name = path.split(PATH_SEPARATOR).at(-1)?.replace(EXE_SUFFIX, "") ?? "";
		onChange({ ...draft, executables: [{ name, path }, ...executables.slice(1)] });
	};

	return (
		<QuickSection number={3} title={t("editor.quick.launchTitle")} hint={t("editor.quick.launchHint")}>
			<div className="grid gap-4 md:grid-cols-2">
				<EditorField id="editor-executable" label={t("editor.quick.executable")} hint={t("editor.quick.executableHint")}>
					<Input
						id="editor-executable"
						className="font-mono text-xs"
						list="editor-executables"
						value={firstExecutable?.path ?? ""}
						placeholder="Game.exe"
						spellCheck={false}
						onChange={(event) => setExecutable(event.currentTarget.value)}
					/>
				</EditorField>
				<EditorField id="editor-executable-name" label={t("editor.quick.executableName")}>
					<Input
						id="editor-executable-name"
						value={firstExecutable?.name ?? ""}
						placeholder={t("editor.quick.executableNamePlaceholder")}
						onChange={(event) => {
							const next = [...executables];
							next[0] = { ...(next[0] ?? { name: "", path: "" }), name: event.currentTarget.value };
							onChange({ ...draft, executables: next });
						}}
					/>
				</EditorField>
			</div>
			{inspection && inspection.executables.length > 0 && (
				<datalist id="editor-executables">
					{inspection.executables.map((executable) => (
						<option key={executable.path} value={executable.path} label={executable.name}>
							{executable.name}
						</option>
					))}
				</datalist>
			)}
			{executables.length > 1 && <p className="text-muted-foreground text-xs">{t("editor.quick.morePrograms")}</p>}
		</QuickSection>
	);
}

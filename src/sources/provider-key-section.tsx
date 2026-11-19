import { Check, ExternalLink } from "lucide-react";
import { useRef, useState } from "react";
import type { SourceAccount, SourceProviderInfo } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Field, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { useTranslation } from "@/i18n";
import type { KeyCheck } from "@/sources/key-check";
import { SettingRow } from "@/views/settings/controls";

interface ProviderKeySectionProps {
	provider: SourceProviderInfo;
	hasKey: boolean;
	/// The account behind the saved key, once it was checked.
	account?: SourceAccount;
	onSave: (key: string) => Promise<boolean>;
	onValidate: (key: string | null) => Promise<KeyCheck>;
	onOpenKeyPage: () => void;
	/// A short title when a provider heading already names the provider.
	title?: string;
	className?: string;
}

/// The API key of a provider, used for its update checks and downloads. The saved key never reaches the interface: the
/// field stays empty and is used only when the user types a new key. The link under the title opens the page where the
/// provider makes the key, when the provider has one.
export function ProviderKeySection({
	provider,
	hasKey,
	account,
	onSave,
	onValidate,
	onOpenKeyPage,
	title,
	className,
}: ProviderKeySectionProps) {
	const { t } = useTranslation();
	const heading = title ?? t("providers.key.title", { name: provider.name });
	const [value, setValue] = useState("");
	const [shown, setShown] = useState(false);
	const [result, setResult] = useState<KeyCheck | null>(null);
	const [checking, setChecking] = useState(false);
	const trimmed = value.trim();
	/// The field text at the moment of a check, so a late result does not describe a key the user has since changed.
	const currentValue = useRef("");

	const check = async () => {
		const requested = trimmed;
		setChecking(true);
		const outcome = await onValidate(requested || null);
		setChecking(false);
		if (currentValue.current.trim() === requested) setResult(outcome);
	};

	const save = async (key: string) => {
		const saved = await onSave(key);
		if (!saved) return;
		setValue("");
		currentValue.current = "";
		setResult(null);
	};

	return (
		<SettingRow
			title={heading}
			className={className}
			help={t("providers.key.hint", { name: provider.name })}
			action={
				provider.has_key_page && (
					<Button variant="link" size="sm" className="h-auto self-start px-0" onClick={onOpenKeyPage}>
						<ExternalLink /> {t("providers.key.openPage", { name: provider.name })}
					</Button>
				)
			}
		>
			<div className="grid gap-3">
				<Field>
					<FieldLabel htmlFor={`${provider.id}-key`} className="sr-only">
						{heading}
					</FieldLabel>
					<InputGroup className="max-w-xl">
						<InputGroupInput
							id={`${provider.id}-key`}
							type={shown ? "text" : "password"}
							className="font-mono text-sm"
							value={value}
							placeholder={hasKey ? t("providers.key.replaceHint") : t("providers.key.placeholder")}
							autoComplete="off"
							spellCheck={false}
							onChange={(event) => {
								setValue(event.currentTarget.value);
								currentValue.current = event.currentTarget.value;
								setResult(null);
							}}
							onKeyDown={(event) => {
								if (event.key === "Enter" && trimmed) void save(trimmed);
							}}
						/>
						<InputGroupAddon align="inline-end">
							<InputGroupButton aria-pressed={shown} onClick={() => setShown(!shown)}>
								{t("providers.key.show")}
							</InputGroupButton>
						</InputGroupAddon>
					</InputGroup>
				</Field>
				<ButtonGroup>
					<Button variant="outline" size="sm" disabled={!trimmed} onClick={() => void save(trimmed)}>
						{t("common.save")}
					</Button>
					<Button variant="ghost" size="sm" disabled={!(trimmed || hasKey) || checking} onClick={() => void check()}>
						<Check /> {t("providers.key.check")}
					</Button>
					{hasKey && (
						<Button variant="ghost" size="sm" onClick={() => void save("")}>
							{t("providers.key.clear")}
						</Button>
					)}
				</ButtonGroup>
				<p className={hasKey ? "text-primary flex items-center gap-2 text-sm font-medium" : "text-muted-foreground text-sm"}>
					{hasKey && <Check aria-hidden className="size-4" />}
					{hasKey ? t("providers.key.saved") : t("providers.key.none")}
				</p>
				{hasKey && account && (
					<p className="text-muted-foreground text-sm">
						{t(account.direct_download ? "providers.key.accountDirect" : "providers.key.accountBrowser", { name: account.name })}
					</p>
				)}
				{result && <p className={result.ok ? "text-win text-sm" : "text-destructive text-sm"}>{result.text}</p>}
			</div>
		</SettingRow>
	);
}

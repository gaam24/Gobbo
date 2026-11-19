import {
	ChevronDown,
	CircleArrowUp,
	Copy,
	Download,
	FolderOpen,
	Gamepad2,
	RefreshCw,
	ShieldCheck,
	SlidersHorizontal,
	Trash2,
	Wrench,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { HelpHint } from "@/components/app/help-hint";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { Item, ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SettingRow } from "@/views/settings/controls";
import { LocationRow } from "@/views/settings/location-row";

const noop = () => {
	/* dev fixture: the mockup does not open or copy anything */
};

const GAME = "Cyberpunk 2077";
const PROFILE = "Default";
const GAME_DIR = "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Cyberpunk 2077";
const LIBRARY = "C:\\Users\\Player\\AppData\\Roaming\\dev.gobbo.modsorganizer\\games\\cyberpunk-2077\\library";
const ARCHIVES = "C:\\Users\\Player\\AppData\\Roaming\\dev.gobbo.modsorganizer\\games\\cyberpunk-2077\\archives";

function SectionHeader({ title, help }: { title: string; help?: string }) {
	return (
		<div className="flex items-center gap-1">
			<h2 className="text-base font-semibold">{title}</h2>
			{help && <HelpHint>{help}</HelpHint>}
		</div>
	);
}

function SettingTabs({ children }: { children: ReactNode }) {
	return (
		<Tabs orientation="vertical" defaultValue="general" className="grid gap-8 md:grid-cols-[11rem_minmax(0,1fr)]">
			{children}
		</Tabs>
	);
}

function GameSettingsMock() {
	const [isolated, setIsolated] = useState(true);
	const [overlay, setOverlay] = useState(true);
	const [args, setArgs] = useState("-skipStartScreen");
	const [env, setEnv] = useState("WINEDLLOVERRIDES=version=n,b");

	return (
		<SettingTabs>
			<TabsList variant="line" className="h-fit w-full items-stretch justify-start gap-1 bg-transparent p-0 md:sticky md:top-6">
				<TabsTrigger value="general" className="h-9 flex-none justify-start px-3">
					General
				</TabsTrigger>
				<TabsTrigger value="launch" className="h-9 flex-none justify-start px-3">
					Launch
				</TabsTrigger>
				<TabsTrigger value="storage" className="h-9 flex-none justify-start px-3">
					Storage
				</TabsTrigger>
				<TabsTrigger value="tools" className="h-9 flex-none justify-start px-3">
					Tools
				</TabsTrigger>
				<TabsTrigger value="advanced" className="h-9 flex-none justify-start px-3">
					Advanced
				</TabsTrigger>
			</TabsList>

			<TabsContent value="general" className="grid content-start gap-6">
				<SectionHeader title="General" help="The game folder and the settings that affect the next deployment." />
				<div className="grid gap-0">
					<SettingRow title="Game folder" help="The folder that holds the game executable. Gobbo reads the game definition from here.">
						<ul className="grid gap-2">
							<LocationRow label="Path" path={GAME_DIR} onOpen={noop} onCopy={noop} />
						</ul>
					</SettingRow>
					<SettingRow
						title="Profile data"
						help="Files and folders that follow the selected profile. Use a single file for a config the game reads outside its folder."
					>
						<div className="grid gap-2">
							<Item variant="outline" size="sm">
								<ItemMedia variant="icon">
									<FolderOpen />
								</ItemMedia>
								<ItemContent>
									<ItemTitle>bin/x64/plugins/cyber_engine_tweaks</ItemTitle>
									<ItemDescription>Folder · copied for each profile</ItemDescription>
								</ItemContent>
								<ItemActions>
									<Button variant="ghost" size="icon-sm" aria-label="Remove">
										<Trash2 />
									</Button>
								</ItemActions>
							</Item>
							<ButtonGroup>
								<Button variant="outline" size="sm">
									Add file
								</Button>
								<Button variant="outline" size="sm">
									Add folder
								</Button>
							</ButtonGroup>
						</div>
					</SettingRow>
					<SettingRow
						title="User data"
						help="Folders the game keeps outside its own folder. Isolated keeps a separate copy for each profile; shared reads the same folder everywhere."
					>
						<Item variant="outline" size="sm">
							<ItemMedia variant="icon">
								<Gamepad2 />
							</ItemMedia>
							<ItemContent>
								<ItemTitle>
									Saves <Badge variant="secondary">Profile</Badge>
								</ItemTitle>
								<ItemDescription className="font-mono">%USERPROFILE%/Saved Games/CD Projekt Red/Cyberpunk 2077</ItemDescription>
							</ItemContent>
							<ItemActions>
								<div className="flex items-center gap-2">
									<Switch checked={isolated} onCheckedChange={setIsolated} aria-label="Keep saves separate" />
									<span className="text-muted-foreground text-xs">Separate</span>
								</div>
							</ItemActions>
						</Item>
					</SettingRow>
				</div>
			</TabsContent>

			<TabsContent value="launch" className="grid content-start gap-6">
				<SectionHeader title="Launch" help="Extra arguments and environment variables are passed only when the game starts from Gobbo." />
				<div className="grid gap-0">
					<SettingRow title="Arguments" htmlFor="mock-args" hint="One argument per line or space-separated.">
						<Input id="mock-args" value={args} onChange={(event) => setArgs(event.currentTarget.value)} />
					</SettingRow>
					<SettingRow
						title="Environment"
						htmlFor="mock-env"
						help="Variables are written into the game process. Keep one NAME=value per line."
					>
						<Textarea
							id="mock-env"
							value={env}
							rows={3}
							className="font-mono text-xs"
							onChange={(event) => setEnv(event.currentTarget.value)}
						/>
					</SettingRow>
					<SettingRow
						title="Steam launch options"
						help="Copy this line into the game's Steam properties if you want Steam to start it with the same arguments."
					>
						<div className="grid gap-2">
							<code className="bg-muted rounded-lg border p-2.5 font-mono text-xs break-all">
								WINEDLLOVERRIDES="version=n,b" %command% -skipStartScreen
							</code>
							<Button variant="outline" size="sm" className="justify-self-start">
								<Copy /> Copy
							</Button>
						</div>
					</SettingRow>
				</div>
			</TabsContent>

			<TabsContent value="storage" className="grid content-start gap-6">
				<SectionHeader
					title="Storage"
					help="Where Gobbo keeps the mod library, archives and per-profile data. Removing the game never deletes these folders."
				/>
				<ul className="grid gap-2">
					<LocationRow label="Game" path={GAME_DIR} onOpen={noop} onCopy={noop} />
					<LocationRow label="Mod library" path={LIBRARY} onOpen={noop} onCopy={noop} />
					<LocationRow label="Archives" path={ARCHIVES} onOpen={noop} onCopy={noop} />
				</ul>
				<div className="flex flex-wrap items-center gap-2">
					<Button variant="outline" size="sm">
						<FolderOpen /> Open data folder
					</Button>
					<Badge variant="outline">12.4 GB used</Badge>
				</div>
			</TabsContent>

			<TabsContent value="tools" className="grid content-start gap-6">
				<SectionHeader
					title="Tools"
					help="Programs that belong to the game, such as script mergers. Gobbo runs them inside the overlay so they see the active profile."
				/>
				<div className="grid gap-3">
					<Item variant="outline" size="sm">
						<ItemMedia variant="icon">
							<Wrench />
						</ItemMedia>
						<ItemContent>
							<ItemTitle>
								WolvenKit <Badge variant="outline">Detected</Badge>
							</ItemTitle>
							<ItemDescription>Runs in the overlay · output: bin/x64</ItemDescription>
						</ItemContent>
						<ItemActions>
							<Button variant="outline" size="sm">
								Open
							</Button>
						</ItemActions>
					</Item>
					<Item variant="outline" size="sm">
						<ItemMedia variant="icon">
							<CircleArrowUp />
						</ItemMedia>
						<ItemContent>
							<ItemTitle>
								Cyber Engine Tweaks <Badge variant="outline">Not found</Badge>
							</ItemTitle>
							<ItemDescription>Not in the game folder · get it from Nexus</ItemDescription>
						</ItemContent>
						<ItemActions>
							<Button variant="outline" size="sm">
								Get from Nexus
							</Button>
						</ItemActions>
					</Item>
				</div>
				<SettingRow
					title="Overlay tools"
					help="Only overlay deployments can run tools against the active profile without touching the game folder."
				>
					<div className="flex items-center gap-2">
						<Switch checked={overlay} onCheckedChange={setOverlay} aria-label="Allow overlay tools" />
						<span className="text-muted-foreground text-sm">Enabled for this profile</span>
					</div>
				</SettingRow>
			</TabsContent>

			<TabsContent value="advanced" className="grid content-start gap-6">
				<SectionHeader title="Advanced" help="Rare actions. They are kept away from the everyday settings." />
				<div className="grid gap-0">
					<SettingRow title="Rescan the game folder" help="Reads the game folder again after files were added or removed outside Gobbo.">
						<Button variant="outline" size="sm" className="justify-self-start">
							<RefreshCw /> Rescan
						</Button>
					</SettingRow>
					<SettingRow title="Delete unused versions" help="Removes installed versions that no profile uses. The current version stays.">
						<Button variant="outline" size="sm" className="justify-self-start">
							<Trash2 /> Delete unused versions
						</Button>
					</SettingRow>
				</div>
				<Item variant="outline" className="border-destructive/30">
					<ItemContent>
						<ItemTitle>Remove {GAME}</ItemTitle>
						<ItemDescription>The game files and the mod library stay on disk. Only the game and its profiles leave Gobbo.</ItemDescription>
					</ItemContent>
					<ItemActions>
						<Button variant="destructive" size="sm">
							<Trash2 /> Remove game
						</Button>
					</ItemActions>
				</Item>
			</TabsContent>
		</SettingTabs>
	);
}

function AppSettingsMock() {
	const [nxm, setNxm] = useState(true);
	const [remember, setRemember] = useState(true);

	return (
		<SettingTabs>
			<TabsList variant="line" className="h-fit w-full items-stretch justify-start gap-1 bg-transparent p-0 md:sticky md:top-6">
				<TabsTrigger value="general" className="h-9 flex-none justify-start px-3">
					Appearance
				</TabsTrigger>
				<TabsTrigger value="nexus" className="h-9 flex-none justify-start px-3">
					Nexus
				</TabsTrigger>
				<TabsTrigger value="storage" className="h-9 flex-none justify-start px-3">
					Downloads
				</TabsTrigger>
				<TabsTrigger value="security" className="h-9 flex-none justify-start px-3">
					Security
				</TabsTrigger>
			</TabsList>

			<TabsContent value="general" className="grid content-start gap-6">
				<SectionHeader title="Appearance" help="These settings apply to every game. Language changes take effect immediately." />
				<div className="grid gap-0">
					<SettingRow
						title="Language"
						htmlFor="mock-language"
						help="System follows your computer. English is used when a translation is missing."
					>
						<Select defaultValue="system">
							<SelectTrigger id="mock-language" className="w-full md:w-64">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="system">System</SelectItem>
								<SelectItem value="en">English</SelectItem>
								<SelectItem value="pl">Polski</SelectItem>
							</SelectContent>
						</Select>
					</SettingRow>
					<SettingRow title="Remember the window" help="The window opens with the size and place it had when it was closed.">
						<div className="flex items-center gap-2">
							<Switch checked={remember} onCheckedChange={setRemember} aria-label="Remember the window" />
							<span className="text-muted-foreground text-sm">Remember size and place</span>
						</div>
					</SettingRow>
				</div>
			</TabsContent>

			<TabsContent value="nexus" className="grid content-start gap-6">
				<SectionHeader title="Nexus" help="Gobbo uses the key only for update checks and downloads. It never leaves this computer." />
				<div className="grid gap-0">
					<SettingRow
						title="API key"
						htmlFor="mock-api-key"
						help="Find it in your Nexus account settings. The saved key is never shown again."
					>
						<InputGroup className="max-w-xl">
							<InputGroupInput id="mock-api-key" type="password" placeholder="Paste your personal API key" />
							<InputGroupAddon align="inline-end">
								<InputGroupButton>Show</InputGroupButton>
							</InputGroupAddon>
						</InputGroup>
					</SettingRow>
					<SettingRow
						title="Download links"
						help="Only one program can take nxm:// links. Turn this off if another mod manager should handle them."
					>
						<div className="flex items-center gap-2">
							<Switch checked={nxm} onCheckedChange={setNxm} aria-label="Take nxm:// links" />
							<span className="text-muted-foreground text-sm">Take nxm:// links</span>
						</div>
					</SettingRow>
				</div>
				<Item variant="outline" size="sm">
					<ItemMedia variant="icon">
						<CircleArrowUp />
					</ItemMedia>
					<ItemContent>
						<ItemTitle>Check for updates</ItemTitle>
						<ItemDescription>Uses the saved key and the mod pages on Nexus.</ItemDescription>
					</ItemContent>
					<ItemActions>
						<Button variant="outline" size="sm">
							Check now
						</Button>
					</ItemActions>
				</Item>
			</TabsContent>

			<TabsContent value="storage" className="grid content-start gap-6">
				<SectionHeader
					title="Downloads"
					help="Files from the Nexus download button land here. They install themselves when the download finishes."
				/>
				<ul className="grid gap-2">
					<LocationRow
						label="Data folder"
						path="C:\\Users\\Player\\AppData\\Roaming\\dev.gobbo.modsorganizer"
						onOpen={noop}
						onCopy={noop}
					/>
				</ul>
				<div className="flex flex-wrap gap-2">
					<Button variant="outline" size="sm">
						<Download /> Open downloads
					</Button>
					<Button variant="ghost" size="sm">
						Clear finished
					</Button>
				</div>
			</TabsContent>

			<TabsContent value="security" className="grid content-start gap-6">
				<SectionHeader title="Security" help="Gobbo never edits a file in the game folder while the overlay is active." />
				<div className="grid gap-4">
					<Item variant="outline">
						<ItemMedia variant="icon">
							<ShieldCheck />
						</ItemMedia>
						<ItemContent>
							<ItemTitle>Managed deployment</ItemTitle>
							<ItemDescription>Every file Gobbo places is recorded. Undo removes only those files.</ItemDescription>
						</ItemContent>
					</Item>
					<Item variant="outline">
						<ItemMedia variant="icon">
							<SlidersHorizontal />
						</ItemMedia>
						<ItemContent>
							<ItemTitle>Overlay by default</ItemTitle>
							<ItemDescription>The game folder stays untouched; the overlay serves the active profile.</ItemDescription>
						</ItemContent>
					</Item>
				</div>
			</TabsContent>
		</SettingTabs>
	);
}

/// Dev-only mockup of the settings redesign. It uses the real primitives and the real `SettingRow`, but no backend.
export function SettingsMockup() {
	const [scope, setScope] = useState<"game" | "app">("game");

	return (
		<TooltipProvider>
			<div className="bg-background text-foreground h-screen overflow-y-auto">
				<main className="mx-auto grid w-full max-w-6xl gap-6 px-6 py-6">
					<header className="flex flex-wrap items-end justify-between gap-4 border-b pb-5">
						<div className="grid gap-1">
							<div className="flex items-center gap-2 text-xs">
								<span className="text-subtle-foreground">{GAME}</span>
								<span className="text-subtle-foreground">/</span>
								<Button variant="outline" size="sm" className="h-7 max-w-48 justify-start gap-1.5">
									{PROFILE} <ChevronDown className="text-muted-foreground size-3.5" />
								</Button>
								<Badge variant="outline">Deployed</Badge>
							</div>
							<h1 className="text-xl font-semibold">Settings</h1>
							<p className="text-muted-foreground text-sm">Short labels first. The details are one hover away.</p>
						</div>
						<Badge variant="secondary">Mockup · dev only</Badge>
					</header>

					<Tabs value={scope} onValueChange={(value) => setScope(value === "app" ? "app" : "game")}>
						<TabsList variant="line" className="w-full justify-start border-b">
							<TabsTrigger value="game" className="flex-none">
								<Gamepad2 /> Game settings
							</TabsTrigger>
							<TabsTrigger value="app" className="flex-none">
								<SlidersHorizontal /> Gobbo settings
							</TabsTrigger>
						</TabsList>
						<TabsContent value="game" className="pt-6">
							<GameSettingsMock />
						</TabsContent>
						<TabsContent value="app" className="pt-6">
							<AppSettingsMock />
						</TabsContent>
					</Tabs>
				</main>
			</div>
		</TooltipProvider>
	);
}

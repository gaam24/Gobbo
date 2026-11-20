import { Bell, Check, FolderOpen, Plus, Search, Trash2 } from "lucide-react";
import { type ReactNode, useState } from "react";
import type { RunReport } from "@/bindings";
import { StatusBadge } from "@/components/app/status-badge";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
	AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Item, ItemActions, ItemContent, ItemDescription, ItemMedia, ItemTitle } from "@/components/ui/item";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { Toaster } from "@/components/ui/sonner";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { toast } from "@/lib/notify";
import { LaunchBody } from "@/views/console/launch-body";
import { SettingRow } from "@/views/settings/controls";
import { LocationRow } from "@/views/settings/location-row";

const noop = () => {
	/* dev fixture: the mockup does not open or copy anything */
};

const COPY = {
	title: "Gobbo UI gallery",
	hint: "Dev-only: every shadcn primitive in the states the app uses.",
	buttons: "Buttons",
	forms: "Forms",
	settingsRow: "Settings row",
	console: "Console tabs",
	selection: "Selection",
	feedback: "Feedback",
	overlays: "Overlays",
	data: "Data",
	typography: "Type and keys",
	empty: "Empty",
};

const FAKE_REPORT: RunReport = {
	started_at: 1_763_000_000,
	finished_at: 1_763_000_060,
	game_started: true,
	game_exit_code: 0,
	steps: [
		{
			id: "prelaunch",
			phase: "before_start",
			status: "done",
			code: "steps.done",
			params: {},
			outputs: [{ path: "Data/SKSE/Plugins/example.log", bytes: 4096 }],
			log_tail: ["[prelaunch] copied 3 files", "[prelaunch] done"],
			started_at: 1_763_000_000,
			finished_at: 1_763_000_001,
		},
	],
	output: [
		"ChromaSDKPlugin failed to load!",
		"File 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\Cyberpunk 2077\\bin\\x64\\sl.common.dll' is signed by NVIDIA and the signature was verified.",
	],
	game_changes: [
		{
			path: "bin/x64/plugins/cyber_engine_tweaks/mods/DriftVolume/DriftVolume-Lua.log",
			kind: "modified",
		},
	],
	game_changes_total: 1,
	decisions: [
		{
			seq: 1,
			time: 1_763_000_000,
			pid: 4242,
			thread: 1,
			kind: "write",
			layer: "upper",
			error: 0,
			path: "bin/x64/plugins/cyber_engine_tweaks/mods/DriftVolume/DriftVolume-Lua.log",
		},
	],
	decisions_total: 1,
	crash_dump: null,
	dlls: {
		layer: ["bin/x64/winmm.dll", "bin/x64/version.dll"],
		injected: [
			{
				name: "WINMM.dll",
				path: "bin/x64/WINMM.dll",
				source: "mods/winmm/bin/x64/WINMM.dll",
			},
		],
	},
};

function Section({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="grid gap-4">
			<h2 className="text-base font-semibold">{title}</h2>
			{children}
		</section>
	);
}

export function UiGallery() {
	const [checked, setChecked] = useState(true);
	const [radio, setRadio] = useState("one");
	const [filter, setFilter] = useState("all");
	const [progress, setProgress] = useState(42);

	return (
		<TooltipProvider>
			<div className="h-screen overflow-y-auto">
				<main className="mx-auto grid max-w-5xl gap-10 px-6 py-8">
					<header className="grid gap-1">
						<h1 className="text-xl font-semibold">{COPY.title}</h1>
						<p className="text-muted-foreground text-sm">{COPY.hint}</p>
					</header>

					<Section title={COPY.buttons}>
						<div className="flex flex-wrap items-center gap-2">
							<Button>Default</Button>
							<Button variant="secondary">Secondary</Button>
							<Button variant="outline">Outline</Button>
							<Button variant="ghost">Ghost</Button>
							<Button variant="destructive">Destructive</Button>
							<Button variant="link">Link</Button>
							<Button disabled>Disabled</Button>
							<Button aria-busy>
								<Spinner data-icon="inline-start" aria-hidden /> Loading
							</Button>
						</div>
						<div className="flex flex-wrap items-center gap-2">
							<Button size="xs">XS</Button>
							<Button size="sm">SM</Button>
							<Button>Default</Button>
							<Button size="lg">LG</Button>
							<Button size="icon" aria-label="Add">
								<Plus />
							</Button>
							<Button size="icon-sm" aria-label="Search">
								<Search />
							</Button>
							<ButtonGroup>
								<Button variant="outline">Open</Button>
								<Button variant="outline">Copy</Button>
							</ButtonGroup>
						</div>
					</Section>

					<Section title={COPY.forms}>
						<FieldGroup className="max-w-xl">
							<Field>
								<FieldLabel htmlFor="gallery-name">Name</FieldLabel>
								<Input id="gallery-name" placeholder="A mod name" />
								<FieldDescription>Shown in the mod list.</FieldDescription>
							</Field>
							<Field data-invalid>
								<FieldLabel htmlFor="gallery-error">Nexus ID</FieldLabel>
								<Input id="gallery-error" aria-invalid defaultValue="abc" />
								<FieldError>Digits only.</FieldError>
							</Field>
							<Field>
								<FieldLabel htmlFor="gallery-note">Note</FieldLabel>
								<Textarea id="gallery-note" rows={3} placeholder="A note" />
							</Field>
							<Field>
								<FieldLabel htmlFor="gallery-select">Sort order</FieldLabel>
								<Select defaultValue="name">
									<SelectTrigger id="gallery-select">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem value="name">Name</SelectItem>
										<SelectItem value="author">Author</SelectItem>
										<SelectItem value="updated">Updated</SelectItem>
									</SelectContent>
								</Select>
							</Field>
							<Field>
								<FieldLabel htmlFor="gallery-search">Search</FieldLabel>
								<InputGroup>
									<InputGroupInput id="gallery-search" placeholder="Search mods" />
									<InputGroupAddon>
										<Search />
									</InputGroupAddon>
								</InputGroup>
							</Field>
						</FieldGroup>
					</Section>

					<Section title={COPY.settingsRow}>
						<div className="grid max-w-2xl gap-4">
							<SettingRow
								title="Additional profile files"
								htmlFor="gallery-setting-switch"
								hint="Files or folders kept separately for each profile by copying them around a launch. Use this for a single file the game reads outside its folder; a whole folder belongs in the game data sets above. Large folders take time."
							>
								<Field orientation="horizontal">
									<Switch id="gallery-setting-switch" checked={checked} onCheckedChange={setChecked} />
									<FieldLabel htmlFor="gallery-setting-switch">Take nxm:// links</FieldLabel>
								</Field>
							</SettingRow>
							<ul className="grid gap-3">
								<LocationRow
									label="Mod library"
									path="C:\\Users\\Player\\AppData\\Roaming\\dev.gobbo.modsorganizer\\games\\the-witcher-3-wild-hunt\\library"
									onOpen={noop}
									onCopy={noop}
								/>
							</ul>
						</div>
					</Section>

					<Section title={COPY.console}>
						<div className="bg-card flex h-96 flex-col overflow-hidden rounded-xl border">
							<LaunchBody report={FAKE_REPORT} gameId="preview" />
						</div>
					</Section>

					<Section title={COPY.selection}>
						<div className="flex flex-wrap items-center gap-6">
							<div className="flex items-center gap-2">
								<Checkbox id="gallery-check" checked={checked} onCheckedChange={(value) => setChecked(value === true)} />
								<Label htmlFor="gallery-check">Checkbox</Label>
							</div>
							<div className="flex items-center gap-2">
								<Switch id="gallery-switch" checked={checked} onCheckedChange={setChecked} />
								<Label htmlFor="gallery-switch">Switch</Label>
							</div>
							<RadioGroup value={radio} onValueChange={setRadio} className="flex gap-4">
								<div className="flex items-center gap-2">
									<RadioGroupItem value="one" id="gallery-radio-one" />
									<Label htmlFor="gallery-radio-one">One</Label>
								</div>
								<div className="flex items-center gap-2">
									<RadioGroupItem value="two" id="gallery-radio-two" />
									<Label htmlFor="gallery-radio-two">Two</Label>
								</div>
							</RadioGroup>
							<ToggleGroup type="single" value={filter} onValueChange={(value) => value && setFilter(value)} spacing={0}>
								<ToggleGroupItem value="all">All</ToggleGroupItem>
								<ToggleGroupItem value="enabled">Enabled</ToggleGroupItem>
								<ToggleGroupItem value="disabled">Disabled</ToggleGroupItem>
							</ToggleGroup>
						</div>
						<Tabs defaultValue="overview" className="max-w-xl">
							<TabsList variant="line">
								<TabsTrigger value="overview">Overview</TabsTrigger>
								<TabsTrigger value="files">Files</TabsTrigger>
							</TabsList>
							<TabsContent value="overview" className="text-muted-foreground text-sm">
								Overview content.
							</TabsContent>
							<TabsContent value="files" className="text-muted-foreground text-sm">
								Files content.
							</TabsContent>
						</Tabs>
					</Section>

					<Section title={COPY.feedback}>
						<Alert>
							<Bell />
							<AlertTitle>Information</AlertTitle>
							<AlertDescription>An informational message.</AlertDescription>
						</Alert>
						<Alert variant="destructive">
							<AlertTitle>Error</AlertTitle>
							<AlertDescription>Something failed.</AlertDescription>
						</Alert>
						<div className="flex flex-wrap items-center gap-2">
							<Badge>Default</Badge>
							<Badge variant="secondary">Secondary</Badge>
							<Badge variant="outline">Outline</Badge>
							<Badge variant="destructive">Destructive</Badge>
							<StatusBadge tone="success" dot>
								Deployed
							</StatusBadge>
							<StatusBadge tone="warning" dot>
								Draft
							</StatusBadge>
							<StatusBadge tone="danger" dot>
								Running
							</StatusBadge>
							<Button variant="outline" onClick={() => toast.add({ type: "success", title: "Saved", timeout: 3000 })}>
								Show toast
							</Button>
						</div>
						<div className="grid max-w-xl gap-3">
							<Progress value={progress} aria-label="Progress" />
							<div className="flex gap-2">
								<Button size="sm" variant="outline" onClick={() => setProgress((value) => Math.max(0, value - 10))}>
									-10
								</Button>
								<Button size="sm" variant="outline" onClick={() => setProgress((value) => Math.min(100, value + 10))}>
									+10
								</Button>
								<Spinner />
							</div>
							<div className="grid gap-2">
								<Skeleton className="h-4 w-48" />
								<Skeleton className="h-4 w-32" />
							</div>
						</div>
					</Section>

					<Section title={COPY.overlays}>
						<div className="flex flex-wrap gap-2">
							<Dialog>
								<DialogTrigger asChild>
									<Button variant="outline">Dialog</Button>
								</DialogTrigger>
								<DialogContent>
									<DialogHeader>
										<DialogTitle>Dialog title</DialogTitle>
										<DialogDescription>Dialog description.</DialogDescription>
									</DialogHeader>
									<DialogFooter>
										<Button variant="outline">Cancel</Button>
										<Button>Confirm</Button>
									</DialogFooter>
								</DialogContent>
							</Dialog>
							<AlertDialog>
								<AlertDialogTrigger asChild>
									<Button variant="outline">Alert dialog</Button>
								</AlertDialogTrigger>
								<AlertDialogContent>
									<AlertDialogHeader>
										<AlertDialogTitle>Delete?</AlertDialogTitle>
										<AlertDialogDescription>This cannot be undone.</AlertDialogDescription>
									</AlertDialogHeader>
									<AlertDialogFooter>
										<AlertDialogCancel>Cancel</AlertDialogCancel>
										<AlertDialogAction>Delete</AlertDialogAction>
									</AlertDialogFooter>
								</AlertDialogContent>
							</AlertDialog>
							<Sheet>
								<SheetTrigger asChild>
									<Button variant="outline">Sheet</Button>
								</SheetTrigger>
								<SheetContent>
									<SheetHeader>
										<SheetTitle>Sheet title</SheetTitle>
										<SheetDescription>Sheet description.</SheetDescription>
									</SheetHeader>
								</SheetContent>
							</Sheet>
							<Popover>
								<PopoverTrigger asChild>
									<Button variant="outline">Popover</Button>
								</PopoverTrigger>
								<PopoverContent>Popover content.</PopoverContent>
							</Popover>
							<Tooltip>
								<TooltipTrigger asChild>
									<Button variant="outline">Tooltip</Button>
								</TooltipTrigger>
								<TooltipContent>Tooltip content</TooltipContent>
							</Tooltip>
						</div>
					</Section>

					<Section title={COPY.data}>
						<Card>
							<CardHeader>
								<CardTitle>Card title</CardTitle>
								<CardDescription>Card description.</CardDescription>
							</CardHeader>
							<CardContent className="grid gap-2">
								<Item variant="outline">
									<ItemMedia variant="icon">
										<FolderOpen />
									</ItemMedia>
									<ItemContent>
										<ItemTitle>Item title</ItemTitle>
										<ItemDescription>Item description.</ItemDescription>
									</ItemContent>
									<ItemActions>
										<Button size="sm" variant="outline">
											Open
										</Button>
									</ItemActions>
								</Item>
								<Separator />
								<Table>
									<TableHeader>
										<TableRow>
											<TableHead>Name</TableHead>
											<TableHead>State</TableHead>
										</TableRow>
									</TableHeader>
									<TableBody>
										<TableRow>
											<TableCell>Mod</TableCell>
											<TableCell>
												<Badge variant="secondary">Enabled</Badge>
											</TableCell>
										</TableRow>
									</TableBody>
								</Table>
							</CardContent>
						</Card>
						<Empty className="border border-dashed">
							<EmptyHeader>
								<EmptyMedia variant="icon">
									<Trash2 />
								</EmptyMedia>
								<EmptyTitle>Empty state</EmptyTitle>
								<EmptyDescription>Nothing here yet.</EmptyDescription>
							</EmptyHeader>
							<EmptyContent>
								<Button size="sm">
									<Plus /> Add
								</Button>
							</EmptyContent>
						</Empty>
					</Section>

					<Section title={COPY.typography}>
						<div className="flex flex-wrap items-center gap-3">
							<KbdGroup>
								<Kbd>Ctrl</Kbd>
								<Kbd>K</Kbd>
							</KbdGroup>
							<span className="font-mono text-xs">C:\Games\Skyrim\Data</span>
							<span className="text-muted-foreground text-sm">Secondary text</span>
							<span className="text-subtle-foreground text-xs">Tertiary text</span>
						</div>
						<div className="flex flex-wrap gap-2">
							<Button variant="ghost" size="icon-sm" aria-label="Check">
								<Check />
							</Button>
							<Button variant="ghost" size="icon-sm" aria-label="Delete">
								<Trash2 />
							</Button>
						</div>
					</Section>
				</main>
			</div>
			<Toaster position="top-right" closeButtonAriaLabel="Dismiss toast" />
		</TooltipProvider>
	);
}

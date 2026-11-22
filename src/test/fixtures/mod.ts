import type { ModView_Serialize } from "@/bindings";

type ModOverrides = Partial<Omit<ModView_Serialize, "info">> & { info?: Partial<ModView_Serialize["info"]> };

/// One installed mod with safe defaults, for tests that need a row without a backend. A test overrides only the fields
/// it reads; `info` is merged too, and naming a folder names the mod unless the test gives its own `info.name`.
export function modFixture(overrides: ModOverrides = {}): ModView_Serialize {
	const base: ModView_Serialize = {
		id: "local:example-mod",
		folder: "Example Mod",
		fomod_installer: false,
		version_count: 1,
		installed_at: 0,
		slot: "Mods",
		enabled: true,
		priority: 1,
		load_priority: null,
		files: 2,
		size_kb: 10,
		wins: 0,
		loses: 0,
		archive: null,
		archive_file: null,
		skipped: 0,
		info: { name: "Example Mod", version: "1.0", author: null, description: null },
		source: null,
		sources: [],
		page_id: null,
		page_name: null,
		part_id: null,
		part_name: null,
		part_category: null,
		update: null,
		tags: [],
		instructions: "",
		unmet_requirements: 0,
		profiles: [],
		collections: [],
		requires_names: [],
		requirement_groups: [],
	};
	const { info, folder } = overrides;
	return {
		...base,
		...overrides,
		info: { ...base.info, name: folder ?? base.info.name, ...info },
	};
}

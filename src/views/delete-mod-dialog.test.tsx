import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { type Mod, pagePartsOf } from "@/lib/mods";
import { DeletePageParts } from "@/views/delete-mod-dialog";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

/// One installed part of a page, with the fields the confirmation lists.
function part(id: string, name: string, version: string | null): Mod {
	return {
		id,
		folder: name,
		fomod_installer: false,
		version_count: 1,
		installed_at: 0,
		slot: "Mods",
		update: null,
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
		info: { name, version, author: null, description: null },
		source: null,
		sources: [],
		page_id: "nexus:225",
		page_name: "KS Page",
		part_id: id,
		part_name: name,
		part_category: null,
		tags: [],
		instructions: "",
		unmet_requirements: 0,
		profiles: [],
		collections: [],
		requires_names: [],
		requirement_groups: [],
	};
}

test("the page confirmation lists every installed part with its version", () => {
	const mods = [
		part("nexus:225", "Base", "1.2"),
		part("nexus:225#200", "Addon", "2.0"),
		{ ...part("nexus:999", "Other page", "3.0"), page_id: "nexus:999" },
	];
	const parts = pagePartsOf(mods, "nexus:225");
	const html = renderToString(<DeletePageParts parts={parts} />);
	expect(parts).toHaveLength(2);
	expect(html).toContain("Base");
	expect(html).toContain("1.2");
	expect(html).toContain("Addon");
	expect(html).toContain("2.0");
	expect(html).not.toContain("Other page");
});

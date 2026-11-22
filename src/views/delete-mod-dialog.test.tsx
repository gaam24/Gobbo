import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { pagePartsOf } from "@/lib/mods";
import { modFixture } from "@/test/fixtures/mod";
import { DeletePageParts } from "@/views/delete-mod-dialog";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

test("the page confirmation lists every installed part with its version", () => {
	const mods = [
		modFixture({
			id: "nexus:225",
			folder: "Base",
			page_id: "nexus:225",
			page_name: "KS Page",
			part_id: "nexus:225",
			part_name: "Base",
			info: { version: "1.2" },
		}),
		modFixture({
			id: "nexus:225#200",
			folder: "Addon",
			page_id: "nexus:225",
			page_name: "KS Page",
			part_id: "nexus:225#200",
			part_name: "Addon",
			info: { version: "2.0" },
		}),
		{ ...modFixture({ id: "nexus:999", folder: "Other page", info: { version: "3.0" } }), page_id: "nexus:999" },
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

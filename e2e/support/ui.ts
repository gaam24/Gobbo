import { browser } from "@wdio/globals";

const SELECTOR_PATTERN = /^[#.[]/;

/// The text the window shows. The suite asserts on the same words a user reads; there is no test-only markup to keep in
/// sync with the UI.
export function bodyText(): Promise<string> {
	return browser.execute(() => document.body.textContent ?? "");
}

export async function waitForText(text: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(async () => (await bodyText()).includes(text), {
		timeout,
		timeoutMsg: `the window did not show ${JSON.stringify(text)}`,
	});
}

/// Waits for a field whose placeholder is the given text. A placeholder is an attribute, not text in the document, so
/// `waitForText` cannot see it.
export async function waitForPlaceholder(text: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string) =>
					[...document.querySelectorAll<HTMLInputElement>("input[placeholder], textarea[placeholder]")].some(
						(field) => field.placeholder === wanted,
					),
				text,
			),
		{
			timeout,
			timeoutMsg: `the field with placeholder ${JSON.stringify(text)} did not appear`,
		},
	);
}

export async function waitForNoText(text: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(async () => !(await bodyText()).includes(text), {
		timeout,
		timeoutMsg: `the window still shows ${JSON.stringify(text)}`,
	});
}

export async function waitForSelector(selector: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(async () => browser.execute((css: string) => document.querySelector(css) !== null, selector), {
		timeout,
		timeoutMsg: `the selector ${selector} did not appear`,
	});
}

/// Waits for the page title. A notice can contain the same word as the page, so the heading is the only reliable screen
/// marker.
export async function waitForHeading(text: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(
		async () => browser.execute((wanted: string) => document.querySelector("h1")?.textContent?.trim() === wanted, text),
		{
			timeout,
			timeoutMsg: `the page heading ${JSON.stringify(text)} did not appear`,
		},
	);
}

export async function waitForNoSelector(selector: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(async () => browser.execute((css: string) => document.querySelector(css) === null, selector), {
		timeout,
		timeoutMsg: `the selector ${selector} did not go away`,
	});
}

export function count(selector: string): Promise<number> {
	return browser.execute((css: string) => document.querySelectorAll(css).length, selector);
}

export async function waitForCount(selector: string, expected: number, timeout = 30_000): Promise<void> {
	await browser.waitUntil(async () => (await count(selector)) === expected, {
		timeout,
		timeoutMsg: `the selector ${selector} did not reach ${expected} elements`,
	});
}

export function textOf(selector: string): Promise<string> {
	return browser.execute((css: string) => document.querySelector(css)?.textContent?.trim() ?? "", selector);
}

/// Clicks the first element whose accessible name is exactly `label`, optionally in a scope (a dialog or a row).
export async function clickByLabel(label: string, scope?: string): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string, selector: string | null) => {
					const root = selector ? document.querySelector(selector) : document;

					if (!root) return false;
					return [...root.querySelectorAll("[aria-label]")].some((element) => element.getAttribute("aria-label") === wanted);
				},
				label,
				scope ?? null,
			),
		{
			timeout: 30_000,
			timeoutMsg: `the control labelled ${JSON.stringify(label)} did not appear`,
		},
	);
	await browser.execute(
		(wanted: string, selector: string | null) => {
			const root = selector ? document.querySelector(selector) : document;
			const element = root
				? [...root.querySelectorAll<HTMLElement>("[aria-label]")].find((item) => item.getAttribute("aria-label") === wanted)
				: undefined;
			element?.click();
		},
		label,
		scope ?? null,
	);
}

/// Opens a Radix tooltip by focusing its trigger and sending it a pointer move. HelpHint keeps its explanation in a
/// tooltip, so the text is absent from the page until the trigger is activated.
export async function openTooltipByLabel(label: string): Promise<void> {
	await browser.execute((wanted: string) => {
		const trigger = [...document.querySelectorAll<HTMLElement>("[aria-label]")].find(
			(candidate) => candidate.getAttribute("aria-label") === wanted,
		);

		if (!trigger) throw new Error(`the tooltip trigger ${wanted} was not found`);
		const box = trigger.getBoundingClientRect();
		trigger.focus();
		trigger.dispatchEvent(
			new PointerEvent("pointermove", {
				bubbles: true,
				cancelable: true,
				pointerType: "mouse",
				pointerId: 1,
				clientX: box.left + 4,
				clientY: box.top + 4,
			}),
		);
	}, label);
}

/// Clicks the first control whose accessible name starts with `prefix`. The notifications button carries the unread
/// count in its label, so its exact text changes with the data.
export async function clickByLabelPrefix(prefix: string, scope?: string): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string, selector: string | null) => {
					const root = selector ? document.querySelector(selector) : document;
					return Boolean(
						root &&
							[...root.querySelectorAll("[aria-label]")].some((element) => (element.getAttribute("aria-label") ?? "").startsWith(wanted)),
					);
				},
				prefix,
				scope ?? null,
			),
		{
			timeout: 30_000,
			timeoutMsg: `the control labelled ${JSON.stringify(prefix)} did not appear`,
		},
	);
	await browser.execute(
		(wanted: string, selector: string | null) => {
			const root = selector ? document.querySelector(selector) : document;
			const element = root
				? [...root.querySelectorAll<HTMLElement>("[aria-label]")].find((item) => (item.getAttribute("aria-label") ?? "").startsWith(wanted))
				: undefined;
			element?.click();
		},
		prefix,
		scope ?? null,
	);
}

/// Clicks the first visible control whose text includes `text`. Use a scope when the same word appears in several places.
export async function clickByText(
	text: string,
	selector = "button, a, [role='menuitem'], [role='option'], [role='tab'], [role='button']",
	scope?: string,
): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string, css: string, scopeSelector: string | null) => {
					const root = scopeSelector ? document.querySelector(scopeSelector) : document;

					if (!root) return false;
					return [...root.querySelectorAll(css)].some((element) => element.textContent?.toLowerCase().includes(wanted.toLowerCase()));
				},
				text,
				selector,
				scope ?? null,
			),
		{
			timeout: 30_000,
			timeoutMsg: `the control with text ${JSON.stringify(text)} did not appear`,
		},
	);
	await browser.execute(
		(wanted: string, css: string, scopeSelector: string | null) => {
			const root = scopeSelector ? document.querySelector(scopeSelector) : document;
			const element = root
				? [...root.querySelectorAll<HTMLElement>(css)].find((item) => item.textContent?.toLowerCase().includes(wanted.toLowerCase()))
				: undefined;
			element?.click();
		},
		text,
		selector,
		scope ?? null,
	);
}

/// Clicks one Radix tab with a native pointer click. A synthetic click reaches the handler for a button but not always for
/// a tab trigger.
export async function clickTab(text: string): Promise<void> {
	const tabs = await browser.$$("[role='tab']");

	for (const tab of tabs) {
		if (!(await tab.isDisplayed())) continue;

		if ((await tab.getText()).toLowerCase().includes(text.toLowerCase())) {
			await tab.click();
			return;
		}
	}

	throw new Error(`the tab ${text} was not found`);
}

/// Clicks a control with a native WebDriver click. Radix dialogs and selects sometimes ignore a synthetic click, so a
/// spec that needs the real pointer path uses this helper instead of `clickByText`.
export async function clickNativeByText(text: string, selector = "button", scope?: string): Promise<void> {
	const elements = await browser.$$(scope ? `${scope} ${selector}` : selector);

	for (const element of elements) {
		if (!(await element.isDisplayed())) continue;

		if ((await element.getText()).toLowerCase().includes(text.toLowerCase())) {
			await element.click();
			return;
		}
	}

	throw new Error(`the control with text ${text} was not found`);
}

/// Clicks the button whose text includes `buttonText` inside the first row/list item whose text includes `rowText`.
/// The Games page keeps its actions in a row, so a name is the only stable scope.
export async function clickInRow(rowText: string, buttonText: string): Promise<void> {
	await browser.execute(
		(row: string, action: string) => {
			const candidates = [...document.querySelectorAll<HTMLElement>("li, [role='row'], [data-grid-card]")].filter((element) =>
				(element.textContent ?? "").includes(row),
			);

			for (const candidate of candidates) {
				const button = [...candidate.querySelectorAll<HTMLElement>("button")].find((element) =>
					(element.textContent ?? "").includes(action),
				);

				if (button) {
					button.click();
					return;
				}
			}

			throw new Error(`the action ${action} was not found in the row ${row}`);
		},
		rowText,
		buttonText,
	);
}

export async function openContextMenuByLabel(label: string): Promise<void> {
	await browser.execute((wanted: string) => {
		const element = [...document.querySelectorAll<HTMLElement>("[aria-label]")].find(
			(candidate) => candidate.getAttribute("aria-label") === wanted,
		);

		if (!element) throw new Error(`the control ${wanted} was not found`);
		const box = element.getBoundingClientRect();
		element.dispatchEvent(
			new MouseEvent("contextmenu", {
				bubbles: true,
				cancelable: true,
				button: 2,
				clientX: box.left + 8,
				clientY: box.top + 8,
			}),
		);
	}, label);
	await waitForSelector("[role='menu']");
}

/// Sets a controlled input or textarea the way a user would, through the native value setter and an input event. A plain
/// `element.value = ...` does not update React state.
export async function typeInto(labelOrSelector: string, value: string): Promise<void> {
	const isSelector = SELECTOR_PATTERN.test(labelOrSelector);
	await browser.execute(
		(target: string, text: string, selector: boolean) => {
			const root = document;
			let element: HTMLInputElement | HTMLTextAreaElement | null = null;

			if (selector) {
				element = root.querySelector<HTMLInputElement | HTMLTextAreaElement>(target);
			} else {
				element =
					[...root.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input, textarea")].find((candidate) => {
						if (candidate.getAttribute("aria-label") === target) return true;

						if ((candidate.placeholder ?? "").includes(target)) return true;

						if (candidate.id) {
							const label = root.querySelector(`label[for="${CSS.escape(candidate.id)}"]`);

							if (label?.textContent?.includes(target)) return true;
						}

						return candidate.closest("label")?.textContent?.includes(target) ?? false;
					}) ?? null;
			}

			if (!element) throw new Error(`the field ${target} was not found`);
			element.focus();
			const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
			const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
			setter?.call(element, text);
			element.dispatchEvent(new Event("input", { bubbles: true }));
			element.dispatchEvent(new Event("change", { bubbles: true }));
		},
		labelOrSelector,
		value,
		isSelector,
	);
}

export async function setSwitch(label: string, on: boolean): Promise<void> {
	await browser.execute(
		(wanted: string, desired: boolean) => {
			const control = [...document.querySelectorAll<HTMLElement>("[role='switch']")].find(
				(element) => element.getAttribute("aria-label") === wanted || element.id === wanted,
			);

			if (!control) throw new Error(`the switch ${wanted} was not found`);
			const checked = control.getAttribute("aria-checked") === "true" || (control as HTMLInputElement).checked;

			if (checked !== desired) control.click();
		},
		label,
		on,
	);
}

export async function press(...keys: string[]): Promise<void> {
	await browser.keys(keys);
}

/// Dispatches a key event on one element. A WebDriver key goes to whatever the window considers focused; a controlled
/// input can lose that focus between two calls, so the keyboard tests use the element the app listens on.
export async function pressKeyOn(selector: string, key: string, modifiers: { control?: boolean; shift?: boolean } = {}): Promise<void> {
	await browser.execute(
		(css: string, pressed: string, ctrl: boolean, shift: boolean) => {
			const element = document.querySelector<HTMLElement>(css) ?? document.body;
			element.focus();
			element.dispatchEvent(
				new KeyboardEvent("keydown", {
					key: pressed,
					bubbles: true,
					cancelable: true,
					ctrlKey: ctrl,
					shiftKey: shift,
				}),
			);
		},
		selector,
		key,
		modifiers.control ?? false,
		modifiers.shift ?? false,
	);
}

/// Dispatches a key on the name button of the mod with this text. The list handler treats that button as the focused row.
export async function pressKeyOnMod(name: string, key: string, modifiers: { control?: boolean; shift?: boolean } = {}): Promise<void> {
	await browser.execute(
		(wanted: string, pressed: string, ctrl: boolean, shift: boolean) => {
			const element = [...document.querySelectorAll<HTMLElement>("[data-mod]")].find((candidate) =>
				(candidate.textContent ?? "").includes(wanted),
			);

			if (!element) throw new Error(`the mod ${wanted} was not found`);
			element.focus();
			element.dispatchEvent(
				new KeyboardEvent("keydown", {
					key: pressed,
					bubbles: true,
					cancelable: true,
					ctrlKey: ctrl,
					shiftKey: shift,
				}),
			);
		},
		name,
		key,
		modifiers.control ?? false,
		modifiers.shift ?? false,
	);
}

/// Opens a Radix dropdown with the pointer sequence its trigger listens to. A plain `element.click()` reaches click but
/// not the pointerdown that opens the menu.
export async function clickMenuTrigger(label: string): Promise<void> {
	// A lingering menu closes on the first pointerdown instead of opening the next one. Close it first.
	if ((await count("[role='menu']")) > 0) {
		await browser.keys("Escape");
		await waitForNoSelector("[role='menu']");
	}

	await browser.execute((wanted: string) => {
		const element = [...document.querySelectorAll<HTMLElement>("[aria-label]")].find(
			(candidate) => candidate.getAttribute("aria-label") === wanted,
		);

		if (!element) throw new Error(`the menu trigger ${wanted} was not found`);
		element.focus();
		const box = element.getBoundingClientRect();

		for (const type of ["pointerdown", "pointerup"] as const) {
			element.dispatchEvent(
				new PointerEvent(type, {
					bubbles: true,
					cancelable: true,
					button: 0,
					pointerId: 1,
					pointerType: "mouse",
					clientX: box.left + 8,
					clientY: box.top + 8,
				}),
			);
		}

		element.click();
	}, label);
	await waitForSelector("[role='menu']");
}

/// Opens a Radix select by its id and picks one option with a native click. A select is not a menu; its items are
/// options in a listbox.
export async function chooseSelect(triggerSelector: string, optionText: string): Promise<void> {
	const opened = await browser.execute((css: string) => {
		const element = document.querySelector<HTMLElement>(css);

		if (!element) return false;
		element.focus();
		element.dispatchEvent(
			new KeyboardEvent("keydown", {
				key: "ArrowDown",
				bubbles: true,
				cancelable: true,
			}),
		);
		return true;
	}, triggerSelector);

	if (!opened) throw new Error(`the select ${triggerSelector} was not found`);
	await waitForSelector("[role='option']");
	// A Radix select acts on the item that has the focus. Focus the wanted option and press Enter, so no pointer path
	// depends on the select's internal item registration.
	const picked = await browser.execute((wanted: string) => {
		const option = [...document.querySelectorAll<HTMLElement>("[role='option']")].find((element) =>
			(element.textContent ?? "").toLowerCase().includes(wanted.toLowerCase()),
		);

		if (!option) return false;
		option.focus();
		option.dispatchEvent(
			new KeyboardEvent("keydown", {
				key: "ArrowDown",
				bubbles: true,
				cancelable: true,
			}),
		);
		option.dispatchEvent(
			new KeyboardEvent("keydown", {
				key: "Enter",
				bubbles: true,
				cancelable: true,
			}),
		);
		return true;
	}, optionText);

	if (!picked) throw new Error(`the option ${optionText} was not found`);
	await waitForNoSelector("[role='option']");
}

export async function waitForDialog(title: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute((wanted: string) => {
				const dialogs = [...document.querySelectorAll("[role='dialog'], [role='alertdialog']")];
				return dialogs.some((dialog) => (dialog.textContent ?? "").includes(wanted));
			}, title),
		{ timeout, timeoutMsg: `the dialog ${JSON.stringify(title)} did not open` },
	);
}

export function dialogCount(): Promise<number> {
	return count("[role='dialog'], [role='alertdialog']");
}

export async function closeTopDialog(): Promise<void> {
	if ((await dialogCount()) === 0) return;
	await press("Escape");
	await waitForNoSelector("[role='dialog'], [role='alertdialog']");
}

export async function waitForMod(name: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string) => [...document.querySelectorAll("[data-mod]")].some((element) => element.textContent?.includes(wanted)),
				name,
			),
		{
			timeout,
			timeoutMsg: `the mod ${JSON.stringify(name)} did not appear in the list`,
		},
	);
}

export async function clickMod(name: string, modifiers: { control?: boolean; shift?: boolean } = {}): Promise<void> {
	await browser.execute(
		(wanted: string, ctrl: boolean, shift: boolean) => {
			const element = [...document.querySelectorAll<HTMLElement>("[data-mod]")].find((candidate) =>
				(candidate.textContent ?? "").includes(wanted),
			);

			if (!element) throw new Error(`the mod ${wanted} was not found`);

			if (ctrl || shift) {
				element.dispatchEvent(
					new MouseEvent("click", {
						bubbles: true,
						cancelable: true,
						ctrlKey: ctrl,
						shiftKey: shift,
					}),
				);
			} else {
				element.click();
			}
		},
		name,
		modifiers.control ?? false,
		modifiers.shift ?? false,
	);
}

export async function toggleMod(name: string): Promise<void> {
	await browser.execute((wanted: string) => {
		const nameElement = [...document.querySelectorAll<HTMLElement>("[data-mod]")].find((candidate) =>
			(candidate.textContent ?? "").includes(wanted),
		);
		const row = nameElement?.closest<HTMLElement>("[role='row'], [data-grid-card], li") ?? nameElement?.parentElement;
		const control = row?.querySelector<HTMLElement>("[role='switch']");
		control?.click();
	}, name);
}

/// Opens a mod's context menu. Radix listens to a real contextmenu event; a synthetic event with the element's box is the
/// closest WebDriver can get without a native right click.
export async function openModMenu(name: string): Promise<void> {
	await browser.execute((wanted: string) => {
		const element = [...document.querySelectorAll<HTMLElement>("[data-mod]")].find((candidate) =>
			(candidate.textContent ?? "").includes(wanted),
		);

		if (!element) throw new Error(`the mod ${wanted} was not found`);
		const box = element.getBoundingClientRect();
		element.dispatchEvent(
			new MouseEvent("contextmenu", {
				bubbles: true,
				cancelable: true,
				button: 2,
				clientX: box.left + 8,
				clientY: box.top + 8,
			}),
		);
	}, name);
	await waitForSelector("[role='menu']");
}

export async function clickMenuItem(text: string): Promise<void> {
	await waitForSelector("[role='menu']");
	// Mark the wanted item in the page, then use one native WebDriver click. Iterating every item with `getText` costs
	// a round trip each and goes stale while a large Columns menu re-renders.
	const marked = await browser.execute((wanted: string) => {
		const item = [...document.querySelectorAll<HTMLElement>("[role='menu'] [data-radix-collection-item]")].find((candidate) =>
			(candidate.textContent ?? "").toLowerCase().includes(wanted.toLowerCase()),
		);

		if (!item) return false;
		item.setAttribute("data-e2e-menu-item", "1");
		return true;
	}, text);

	if (!marked) throw new Error(`the menu item ${text} was not found`);
	const item = await browser.$("[data-e2e-menu-item='1']");
	await item.click();
	await browser.execute(() => {
		document.querySelector("[data-e2e-menu-item='1']")?.removeAttribute("data-e2e-menu-item");
	});
}

/// Activates a Radix menu item with the keyboard. Some menus do not act on a synthetic pointer click; focusing the item
/// and sending Enter is the path their roving focus handles.
export async function activateMenuItem(text: string): Promise<void> {
	await waitForSelector("[role='menu']");
	const found = await browser.execute((wanted: string) => {
		const items = [...document.querySelectorAll<HTMLElement>("[role='menu'] [data-radix-collection-item]")];
		const item = items.find((candidate) => (candidate.textContent ?? "").toLowerCase().includes(wanted.toLowerCase()));

		if (!item) return false;
		item.focus();
		item.dispatchEvent(
			new KeyboardEvent("keydown", {
				key: "Enter",
				bubbles: true,
				cancelable: true,
			}),
		);
		return true;
	}, text);

	if (!found) throw new Error(`the menu item ${text} was not found`);
}

export async function setThrow(view: string | null): Promise<void> {
	await browser.execute((value: string | null) => {
		window.__GOBBO_E2E_THROW__ = value;

		if (value === null) sessionStorage.removeItem("gobbo.e2e.throw");
		else sessionStorage.setItem("gobbo.e2e.throw", value);
	}, view);
}

export async function clearGobboStorage(): Promise<void> {
	await browser.execute(() => {
		for (const key of Object.keys(localStorage)) {
			if (key.startsWith("gobbo.")) localStorage.removeItem(key);
		}
	});
}

export function railView(): Promise<string | null> {
	return browser.execute(() => document.querySelector("nav [aria-current='page']")?.getAttribute("aria-label") ?? null);
}

export async function clickGame(name: string): Promise<void> {
	await clickByLabel(name);
	await waitForText(name);
}

export async function waitForGame(name: string, timeout = 30_000): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string) => [...document.querySelectorAll("[aria-label]")].some((element) => element.getAttribute("aria-label") === wanted),
				name,
			),
		{
			timeout,
			timeoutMsg: `the game ${JSON.stringify(name)} did not appear in the rail`,
		},
	);
}

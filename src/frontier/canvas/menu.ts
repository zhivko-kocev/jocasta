/// <reference lib="dom" />
// The menus whiteboard's canvas asks its host for (`ReviewCanvasUi.showMenu`, 0393), keyboard-first (UX-7).
import type { ReviewDisposable, ReviewMenuRequest } from "./contract.ts";
import { disposable } from "./source.ts";

/** Where a key moves in a menu of `n` enabled items, from item `at`. */
export const MENU_MOVES: Readonly<Record<string, (at: number, n: number) => number>> = {
	ArrowDown: (at, n) => (at + 1) % n,
	ArrowUp: (at, n) => (at - 1 + n) % n,
	Home: () => 0,
	End: (_at, n) => n - 1,
};

/**
 * A menu the canvas asks for, under its anchor and inside the window. It takes the focus on its first enabled item;
 * the arrows move (wrapping), Home and End jump, Tab closes it, and Escape or a choice closes it and gives the focus
 * back to the anchor (UX-7).
 */
export function showMenu(request: ReviewMenuRequest): ReviewDisposable {
	const menu = document.createElement("div");
	menu.className = "wb-menu";
	menu.setAttribute("role", "menu");
	const box = request.anchor.getBoundingClientRect();
	menu.style.top = `${box.bottom + 4}px`;
	let open = true;
	const close = () => {
		if (!open) return;
		open = false;
		menu.remove();
		document.removeEventListener("pointerdown", outside, true);
		document.removeEventListener("keydown", onKey, true);
		request.onHide();
	};
	const outside = (event: Event) => {
		if (!(event.target instanceof Node) || !menu.contains(event.target)) close();
	};
	const buttons: HTMLButtonElement[] = [];
	const enabled = () => buttons.filter((b) => !b.disabled);
	const onKey = (event: KeyboardEvent) => {
		if (event.key === "Escape") {
			event.stopPropagation();
			event.preventDefault();
			close();
			request.anchor.focus();
			return;
		}
		if (event.key === "Tab") {
			// The focus leaves from the anchor, as if the menu had never opened.
			close();
			request.anchor.focus();
			return;
		}
		const items = enabled();
		const move = MENU_MOVES[event.key];
		if (!move || items.length === 0 || !(event.target instanceof Node) || !menu.contains(event.target)) return;
		event.preventDefault();
		items[move(items.indexOf(event.target as HTMLButtonElement), items.length)]?.focus();
	};
	for (const item of request.items) {
		const button = document.createElement("button");
		button.type = "button";
		button.setAttribute("role", item.checked === undefined ? "menuitem" : "menuitemcheckbox");
		if (item.checked !== undefined) button.setAttribute("aria-checked", String(item.checked));
		button.disabled = item.enabled === false;
		button.textContent = item.label;
		button.addEventListener("click", () => {
			close();
			request.anchor.focus();
			void request.onSelect(item.id);
		});
		buttons.push(button);
		menu.append(button);
	}
	document.body.append(menu);
	menu.style.left = `${Math.max(8, Math.min(box.left, innerWidth - menu.offsetWidth - 8))}px`;
	document.addEventListener("pointerdown", outside, true);
	document.addEventListener("keydown", onKey, true);
	enabled()[0]?.focus();
	return disposable(close);
}

// Jocasta brief page. Plain ES module; CSP forbids inline script/style, so all
// presentation goes through classes. Text is only ever written via textContent.
const SVG_NS = "http://www.w3.org/2000/svg";
const REDUCED = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function el(tag, cls, text) {
	const n = document.createElement(tag);
	if (cls) n.className = cls;
	if (text !== undefined && text !== null) n.textContent = String(text);
	return n;
}

function svg(tag, attrs, cls) {
	const n = document.createElementNS(SVG_NS, tag);
	if (cls) n.setAttribute("class", cls);
	for (const k in attrs || {}) n.setAttribute(k, String(attrs[k]));
	return n;
}

function safeUrl(u) {
	if (typeof u !== "string") return null;
	try {
		const p = new URL(u);
		return p.protocol === "http:" || p.protocol === "https:" ? p.href : null;
	} catch (_) {
		return null;
	}
}

function link(url, text, cls) {
	const href = safeUrl(url);
	if (!href) return el("span", cls, text);
	const a = el("a", cls, text);
	a.setAttribute("href", href);
	a.setAttribute("rel", "noopener noreferrer");
	a.setAttribute("target", "_blank");
	return a;
}

function badge(text, cls) {
	return el("span", `badge ${cls || ""}`, text);
}

function table(headers, rows, cellFn) {
	const t = el("table");
	if (headers) {
		const tr = el("tr");
		for (const h of headers) tr.append(el("th", null, h));
		const thead = el("thead");
		thead.append(tr);
		t.append(thead);
	}
	const tb = el("tbody");
	for (const r of rows || []) {
		const tr = el("tr");
		cellFn(r, tr);
		tb.append(tr);
	}
	t.append(tb);
	return t;
}

// ---- section renderers: each returns a node; `ctx.cleanups` collects teardown fns ----

function renderText(s) {
	const box = el("div", "text");
	for (const para of String(s.body || "").split(/\n\s*\n/)) {
		if (para.trim()) box.append(el("p", null, para.trim()));
	}
	return box;
}

function renderFacts(s) {
	const dl = el("dl", "facts");
	for (const r of s.rows || []) {
		dl.append(el("dt", null, r.label));
		const dd = el("dd");
		dd.append(r.url ? link(r.url, r.value) : document.createTextNode(String(r.value)));
		dl.append(dd);
	}
	return dl;
}

function renderChecks(s) {
	return table(["Command", "Exit", "Time"], s.rows, (r, tr) => {
		tr.append(el("td", "mono", r.command));
		tr.append(el("td", r.exit === 0 ? "ok" : "bad", r.exit));
		tr.append(el("td", "num", `${r.ms} ms`));
	});
}

function renderList(s) {
	const ul = el("ul", "list");
	for (const r of s.rows || []) {
		const li = el("li");
		li.append(r.url ? link(r.url, r.text) : document.createTextNode(String(r.text)));
		if (r.note) li.append(el("span", "muted note", ` ${r.note}`));
		ul.append(li);
	}
	return ul;
}

function renderDiff(s) {
	const box = el("div", "diff");
	for (const f of s.files || []) {
		const det = el("details", "file");
		det.open = true;
		det.append(el("summary", "mono", f.path));
		for (const h of f.hunks || []) {
			det.append(el("div", "hunk-header mono", h.header));
			const pre = el("div", "hunk mono");
			for (const l of h.lines || []) {
				const cls = l.kind === "+" ? "add" : l.kind === "-" ? "del" : "ctx";
				const row = el("div", `dl ${cls}`);
				row.append(el("span", "ln", l.old == null ? "" : l.old));
				row.append(el("span", "ln", l.new == null ? "" : l.new));
				row.append(el("span", "sign", l.kind === " " ? "" : l.kind));
				row.append(el("span", "code", l.text));
				pre.append(row);
			}
			det.append(pre);
		}
		box.append(det);
	}
	return box;
}

function renderCards(s) {
	const box = el("div", "cards");
	for (const c of s.cards || []) {
		const card = el("article", `card sev-${String(c.severity).toLowerCase()}`);
		const head = el("div", "card-head");
		head.append(badge(c.severity, `sev ${String(c.severity).toLowerCase()}`));
		head.append(el("span", "mono loc", `${c.file}:${c.line}`));
		head.append(badge(c.verdict, `verdict v-${c.verdict}`));
		const ok = c.proof?.ok;
		head.append(badge(ok ? `checked at ${String(c.proof.sha).slice(0, 7)}` : "not checked", ok ? "good" : "fail"));
		card.append(head);
		if (c.quote) card.append(el("pre", "quote mono", c.quote));
		card.append(el("p", "card-text", c.edited != null && c.edited !== "" ? c.edited : c.text));
		if (c.published?.url) {
			const p = el("p", "published");
			p.append(link(c.published.url, "Published"));
			if (c.published.at_ms) p.append(el("span", "muted", ` ${new Date(c.published.at_ms).toLocaleString()}`));
			card.append(p);
		}
		box.append(card);
	}
	return box;
}

// ---- flow ----

// The Prev / Play / Next walk through a flow's nodes in layer order, and its caption.
function flowControls(sorted, els, ctx) {
	// controls + caption
	const bar = el("div", "flow-controls");
	const prev = el("button", null, "Prev");
	const play = el("button", null, "Play");
	const next = el("button", null, "Next");
	prev.setAttribute("aria-label", "Previous step");
	play.setAttribute("aria-label", "Play or pause walkthrough");
	next.setAttribute("aria-label", "Next step");
	for (const b of [prev, play, next]) b.type = "button";
	bar.append(prev, play, next);
	const caption = el("p", "caption muted", "Press Play to walk through the steps.");
	caption.setAttribute("aria-live", "polite");

	let idx = -1;
	let timer = null;
	function focusAt(i) {
		if (!sorted.length) return;
		idx = (i + sorted.length) % sorted.length;
		const cur = sorted[idx];
		for (const [id, d] of els) d.classList.toggle("focus", id === cur.id);
		caption.classList.remove("muted");
		caption.textContent = "";
		caption.append(el("strong", null, cur.label));
		if (cur.group) caption.append(el("span", "muted", ` (${cur.group})`));
		caption.append(document.createTextNode(` [${cur.state}]${cur.detail ? `: ${cur.detail}` : ""}`));
	}
	function stop() {
		if (timer !== null) clearInterval(timer);
		timer = null;
		play.textContent = "Play";
	}
	play.addEventListener("click", () => {
		if (timer !== null) return stop();
		play.textContent = "Pause";
		focusAt(idx + 1);
		timer = setInterval(() => focusAt(idx + 1), 700);
	});
	prev.addEventListener("click", () => {
		stop();
		focusAt(idx < 0 ? sorted.length - 1 : idx - 1);
	});
	next.addEventListener("click", () => {
		stop();
		focusAt(idx + 1);
	});
	ctx.cleanups.push(stop);
	return { bar, caption };
}

function renderFlow(s, ctx) {
	const nodes = s.nodes || [];
	const edges = s.edges || [];
	const wrap = el("div", "flow");
	const stage = el("div", "flow-stage");
	const grid = el("div", "flow-grid");
	const overlay = svg("svg", {}, "flow-edges");
	overlay.setAttribute("aria-hidden", "true");
	stage.append(overlay, grid);

	const maxLayer = nodes.reduce((m, n) => Math.max(m, n.layer | 0), 0);
	const cols = [];
	for (let i = 0; i <= maxLayer; i++) {
		const c = el("div", "flow-col");
		cols.push(c);
		grid.append(c);
	}
	const els = new Map();
	const sorted = nodes.slice().sort((a, b) => a.layer - b.layer || a.order - b.order);
	for (const n of sorted) {
		const d = el("div", `node state-${n.state} layer-${Math.min(n.layer | 0, 40)}`);
		d.append(el("div", "node-label", n.label));
		if (n.group) d.append(el("div", "node-group muted", n.group));
		cols[n.layer | 0].append(d);
		els.set(n.id, d);
	}

	const { bar, caption } = flowControls(sorted, els, ctx);
	wrap.append(stage, bar, caption);

	const paths = [];
	function layoutEdges() {
		const box = stage.getBoundingClientRect();
		overlay.setAttribute("width", box.width);
		overlay.setAttribute("height", box.height);
		overlay.setAttribute("viewBox", `0 0 ${box.width} ${box.height}`);
		overlay.replaceChildren();
		paths.length = 0;
		const meta = new Map(nodes.map((n) => [n.id, n]));
		for (const e of edges) {
			const a = els.get(e.from),
				b = els.get(e.to);
			if (!a || !b) continue;
			const ra = a.getBoundingClientRect(),
				rb = b.getBoundingClientRect();
			const x1 = ra.right - box.left,
				y1 = ra.top + ra.height / 2 - box.top;
			const x2 = rb.left - box.left,
				y2 = rb.top + rb.height / 2 - box.top;
			const forward = meta.get(e.to).layer > meta.get(e.from).layer;
			let c1x, c1y, c2x, c2y;
			if (forward) {
				const dx = Math.max(24, (x2 - x1) / 2);
				c1x = x1 + dx;
				c1y = y1;
				c2x = x2 - dx;
				c2y = y2;
			} else {
				const off = -Math.max(40, Math.abs(y2 - y1) / 2 + 40);
				c1x = x1 + 70;
				c1y = y1 + off;
				c2x = x2 - 70;
				c2y = y2 + off;
			}
			const p = svg("path", { d: `M${x1} ${y1} C${c1x} ${c1y} ${c2x} ${c2y} ${x2} ${y2}` }, "edge");
			overlay.append(p);
			const len = p.getTotalLength();
			p.setAttribute("stroke-dasharray", len);
			p.setAttribute("stroke-dashoffset", len);
			if (drawn) p.classList.add("drawn");
			paths.push(p);
			if (e.label) {
				const t = svg(
					"text",
					{ x: (x1 + 3 * c1x + 3 * c2x + x2) / 8, y: (y1 + 3 * c1y + 3 * c2y + y2) / 8 - 4 },
					"edge-label",
				);
				t.textContent = e.label;
				overlay.append(t);
			}
		}
	}
	let drawn = REDUCED;
	function drawEdges() {
		drawn = true;
		for (const p of paths) p.classList.add("drawn");
	}

	// fade in layer by layer after first paint, then draw edges
	ctx.afterAttach.push(() => {
		layoutEdges();
		const showAll = () => {
			for (const d of els.values()) d.classList.add("shown");
		};
		if (REDUCED) {
			showAll();
		} else {
			requestAnimationFrame(() =>
				requestAnimationFrame(() => {
					showAll();
					const t = setTimeout(drawEdges, maxLayer * 120 + 400);
					ctx.cleanups.push(() => clearTimeout(t));
				}),
			);
		}
		const onResize = () => layoutEdges();
		window.addEventListener("resize", onResize);
		ctx.cleanups.push(() => window.removeEventListener("resize", onResize));
	});
	return wrap;
}

// ---- map ----

function renderMap(s, _ctx) {
	const groups = s.groups || [];
	const links = s.links || [];
	const max = Math.max(1, ...groups.flatMap((g) => (g.items || []).map((i) => i.added + i.removed)));
	const BAR = 120;
	const wrap = el("div", "map");
	const stage = el("div", "map-stage");
	const overlay = svg("svg", {}, "map-arcs");
	overlay.setAttribute("aria-hidden", "true");
	const cols = el("div", "map-cols");
	stage.append(overlay, cols);
	wrap.append(stage);

	const items = new Map();
	for (const g of groups) {
		const col = el("section", "map-group");
		col.append(el("h3", null, g.label));
		for (const it of g.items || []) {
			const row = el("div", "map-item");
			row.tabIndex = 0;
			row.append(el("span", "map-label mono", it.label));
			const w = (v) => (v / max) * BAR;
			const bars = svg("svg", { width: BAR, height: 10, viewBox: `0 0 ${BAR} 10` }, "map-bars");
			bars.append(svg("rect", { x: 0, y: 1, width: w(it.added), height: 8 }, "bar-add"));
			bars.append(svg("rect", { x: w(it.added), y: 1, width: w(it.removed), height: 8 }, "bar-del"));
			row.append(bars);
			row.append(el("span", "map-num muted", `+${it.added} -${it.removed}`));
			col.append(row);
			items.set(it.id, row);
		}
		cols.append(col);
	}

	function clear() {
		overlay.replaceChildren();
		for (const r of items.values()) r.classList.remove("linked");
	}
	function show(id) {
		clear();
		const box = stage.getBoundingClientRect();
		overlay.setAttribute("width", box.width);
		overlay.setAttribute("height", box.height);
		overlay.setAttribute("viewBox", `0 0 ${box.width} ${box.height}`);
		for (const l of links) {
			if (l.a !== id && l.b !== id) continue;
			const other = l.a === id ? l.b : l.a;
			const ea = items.get(id),
				eb = items.get(other);
			if (!ea || !eb) continue;
			eb.classList.add("linked");
			const ra = ea.getBoundingClientRect(),
				rb = eb.getBoundingClientRect();
			const left = ra.left <= rb.left;
			const x1 = (left ? ra.right : ra.left) - box.left,
				y1 = ra.top + ra.height / 2 - box.top;
			const x2 = (left ? rb.left : rb.right) - box.left,
				y2 = rb.top + rb.height / 2 - box.top;
			const bend = Math.max(30, Math.abs(x2 - x1) / 2) * (left ? 1 : -1);
			overlay.append(svg("path", { d: `M${x1} ${y1} C${x1 + bend} ${y1} ${x2 - bend} ${y2} ${x2} ${y2}` }, "arc"));
		}
	}
	for (const [id, row] of items) {
		row.addEventListener("mouseenter", () => show(id));
		row.addEventListener("mouseleave", clear);
		row.addEventListener("focus", () => show(id));
		row.addEventListener("blur", clear);
	}
	return wrap;
}

const RENDERERS = {
	text: renderText,
	facts: renderFacts,
	checks: renderChecks,
	flow: renderFlow,
	map: renderMap,
	diff: renderDiff,
	cards: renderCards,
	list: renderList,
};

function renderSection(s, ctx) {
	const sec = el("section", `section type-${String(s?.type).replace(/[^a-z]/g, "")}`);
	if (s && typeof s.heading === "string" && s.heading) sec.append(el("h2", null, s.heading));
	const fn = s && RENDERERS[s.type];
	if (!fn) {
		sec.append(el("p", "muted", "unsupported section"));
		return sec;
	}
	try {
		sec.append(fn(s, ctx));
	} catch (err) {
		sec.append(el("p", "muted", `could not render section: ${err?.message ? err.message : err}`));
	}
	return sec;
}

// ---- actions (thread briefs) ----

function renderActions(brief, onBrief) {
	const bar = el("div", "actions");
	const merge = el("button", "primary", "Merge");
	const pr = el("button", null, "Open PR");
	const text = el("textarea");
	text.rows = 3;
	text.setAttribute("aria-label", "Message to send back to the thread");
	text.setAttribute("placeholder", "Send the agent back with instructions...");
	const send = el("button", null, "Send back");
	for (const b of [merge, pr, send]) b.type = "button";
	const status = el("p", "status muted");
	status.setAttribute("aria-live", "polite");
	const banner = el("p", "banner");
	banner.setAttribute("role", "alert");
	banner.hidden = true;
	const btns = [merge, pr, send];

	async function act(body, waiting) {
		for (const b of btns) b.disabled = true;
		banner.hidden = true;
		status.textContent = waiting ? "Waiting for your Allow in Jocasta..." : "Sending...";
		try {
			const res = await fetch(`/brief/${encodeURIComponent(brief.id)}/act`, {
				method: "POST",
				credentials: "same-origin",
				headers: { "content-type": "application/json" },
				body: JSON.stringify(body),
			});
			const data = await res.json().catch(() => null);
			if (!res.ok) throw new Error(data?.error || `request failed (${res.status})`);
			onBrief(data);
		} catch (err) {
			banner.textContent = err?.message ? err.message : String(err);
			banner.hidden = false;
			status.textContent = "";
			for (const b of btns) b.disabled = false;
		}
	}
	merge.addEventListener("click", () => act({ op: "finish", mode: "merge" }, true));
	pr.addEventListener("click", () => act({ op: "finish", mode: "pr" }, true));
	send.addEventListener("click", () => {
		const t = text.value.trim();
		if (t) void act({ op: "send", text: t }, false);
	});
	const row = el("div", "action-row");
	row.append(merge, pr);
	const sendRow = el("div", "action-send");
	sendRow.append(text, send);
	bar.append(row, sendRow, status, banner);
	return bar;
}

// ---- page ----

function renderBrief(root, brief, ctx) {
	for (const f of ctx.cleanups.splice(0)) f();
	ctx.afterAttach.length = 0;
	const frag = document.createDocumentFragment();
	const header = el("header");
	header.append(el("h1", null, brief.title));
	if (brief.subtitle) header.append(el("p", "subtitle", brief.subtitle));
	header.append(el("p", "muted generated", `generated at ${new Date(brief.generated_at_ms).toLocaleString()}`));
	frag.append(header);
	if (brief.kind === "thread") {
		frag.append(renderActions(brief, (b) => renderBrief(root, b, ctx)));
	}
	for (const s of brief.sections || []) frag.append(renderSection(s, ctx));
	root.replaceChildren(frag);
	document.title = `${brief.title} - Jocasta brief`;
	for (const f of ctx.afterAttach.splice(0)) f();
}

async function main() {
	const root = document.getElementById("brief");
	const id = location.pathname.split("/").filter(Boolean).pop() || "";
	const ctx = { cleanups: [], afterAttach: [] };
	try {
		const res = await fetch(`/brief/${encodeURIComponent(id)}.json`, { credentials: "same-origin" });
		if (!res.ok) {
			const data = await res.json().catch(() => null);
			throw new Error(data?.error || `could not load brief (${res.status})`);
		}
		renderBrief(root, await res.json(), ctx);
	} catch (err) {
		root.replaceChildren(el("p", "banner", err?.message ? err.message : String(err)));
	}
}

void main();

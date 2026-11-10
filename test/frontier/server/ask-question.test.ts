import { expect, test } from "bun:test";
import { askQuestion, formQuestion } from "../../../src/frontier/server/ask-question.ts";

// Each expectation is what omp 18.18.6's own `fieldQuestions` (pi-tui `html-form/data.ts:127-146`) gives for the
// form's first named field, checked against omp's parser when these cases were written (0589).

test("omp's own example form: the radio group's fieldset legend, not the context before it", () => {
	// omp 18.18.6 `tools/ask.ts:335-344`, `EXAMPLE_FORM`.
	const html = `<form>
  <p>Sessions live in memory and are lost on restart.</p>
  <fieldset>
    <legend>Session store</legend>
    <label><input type="radio" name="store" value="sqlite" checked> SQLite: one file, no new service</label>
    <label><input type="radio" name="store" value="redis"> Redis: shared across instances, needs ops</label>
  </fieldset>
  <label>Session lifetime (days) <input type="number" name="ttl_days" value="30" min="1"></label>
</form>`;
	expect(formQuestion(html)).toBe("Session store");
});

test("a field's own label comes first: wrapping, <label for>, aria-label, title", () => {
	expect(
		formQuestion(
			`<form><p>Context first.</p><label>What should the branch be called? <input type="text" name="branch" value="fix/x"></label></form>`,
		),
	).toBe("What should the branch be called?");
	expect(formQuestion(`<form><label for="t">Ticket id</label><input id="t" name="ticket"></form>`)).toBe("Ticket id");
	expect(
		formQuestion(`<form><select name="env" aria-label="Target environment"><option>prod</option></select></form>`),
	).toBe("Target environment");
	expect(formQuestion(`<form><textarea name="msg" title="Commit message"></textarea></form>`)).toBe("Commit message");
	// A select's options are not its label's text.
	expect(
		formQuestion(
			`<form><label>Target <select name="env"><option>prod</option><option>dev</option></select></label></form>`,
		),
	).toBe("Target");
});

test("a group without its own label takes the text block just before it", () => {
	const group = `<label><input type="radio" name="db" value="a" checked> A</label><label><input type="radio" name="db" value="b"> B</label>`;
	expect(formQuestion(`<form><p>Which database should the cache use?</p>${group}</form>`)).toBe(
		"Which database should the cache use?",
	);
	expect(
		formQuestion(`<form><h3>Deploy now?</h3><select name="go"><option>Yes</option><option>No</option></select></form>`),
	).toBe("Deploy now?");
	expect(formQuestion(`<form>Choose one: ${group}</form>`)).toBe("Choose one:");
	// The last text block before the group, not every text before it; a line break inside a block is a space.
	expect(formQuestion(`<form><p>Sessions are lost on restart.</p><p>Which store keeps them?</p>${group}</form>`)).toBe(
		"Which store keeps them?",
	);
	expect(formQuestion(`<form><div>Line one<br>Line two</div>${group}</form>`)).toBe("Line one Line two");
	// A hidden input is no field; an unnamed control is, and ends the caption.
	expect(formQuestion(`<form><input type="hidden" name="k" value="1"><p>Ship it?</p>${group}</form>`)).toBe("Ship it?");
	expect(formQuestion(`<form><p>Note</p><input type="text">${group}</form>`)).toBe("db");
	// A list or table without controls is context between the text and the group: no caption.
	expect(formQuestion(`<form><p>Findings</p><ul><li>one</li><li>two</li></ul>${group}</form>`)).toBe("db");
	// A list of controls is layout: the text before it stays the question.
	expect(
		formQuestion(
			`<form><p>Pick a store</p><ul><li><label><input type="radio" name="s" value="a"> A</label></li><li><label><input type="radio" name="s" value="b"> B</label></li></ul></form>`,
		),
	).toBe("Pick a store");
});

test("then the fieldset's legend, entities decoded", () => {
	expect(
		formQuestion(
			`<form><fieldset><legend>Keep &quot;main&quot; &amp; rebase?</legend><label><input type="radio" name="k" value="y"> Yes</label><label><input type="radio" name="k" value="n"> No</label></fieldset></form>`,
		),
	).toBe('Keep "main" & rebase?');
});

test("then a lone checkbox's or radio's label, then the field's name", () => {
	expect(
		formQuestion(`<form><label><input type="checkbox" name="force" value="1"> Force-push the branch</label></form>`),
	).toBe("Force-push the branch");
	expect(formQuestion(`<form><label><input type="radio" name="only" value="x"> Only choice</label></form>`)).toBe(
		"Only choice",
	);
	// A text field without a label is labelled by its name, `_` and `-` as spaces; a group keeps its name as is.
	expect(formQuestion(`<form><p>Ship it?</p><input type="text" name="ship_note"></form>`)).toBe("ship note");
	expect(
		formQuestion(
			`<form><label><input type="radio" name="db_kind" value="a"> A</label><label><input type="radio" name="db_kind" value="b"> B</label></form>`,
		),
	).toBe("db_kind");
});

test("a form without a named control has no question", () => {
	expect(formQuestion(`<form><p>Just text</p><input type="submit" name="go"></form>`)).toBeNull();
});

test("an ask call's arguments: omp 18.18.6's form, or an older session's questions[]", () => {
	expect(
		askQuestion({
			html: `<form><p>CSV or JSON?</p><label><input type="radio" name="f" value="csv" checked> CSV</label><label><input type="radio" name="f" value="json"> JSON</label></form>`,
		}),
	).toBe("CSV or JSON?");
	// omp 18.18.2's shape (`tools/ask.ts:86-87`).
	expect(
		askQuestion({ questions: [{ question: "CSV or JSON?", options: [{ label: "CSV" }] }, { question: "Later" }] }),
	).toBe("CSV or JSON?");
	expect(askQuestion({ questions: [] })).toBeNull();
	expect(askQuestion({})).toBeNull();
	expect(askQuestion(undefined)).toBeNull();
});

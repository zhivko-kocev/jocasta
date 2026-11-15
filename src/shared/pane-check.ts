// The check a project pane's agent gets when its turn ends (0617): the words the omp extension, the internal role and
// the service share. The extension itself imports nothing (omp loads it from the shipped config folder), so it repeats
// `PANE_CHECK_ENV` and the time limits below; `test/config/pane-check-extension.test.ts` holds them together.

/** The internal word of the `jocasta` command the extension runs when a turn ends (0617); not in the usage text. */
export const PANE_CHECK_ROLE = "pane-check";

/** Where the role asks the service (`POST`, the API token as the bearer): body `{paneKey}`, answered `{text}`. */
export const PANE_CHECK_PATH = "/api/pane-check";

/** What the service answers (`PANE_CHECK_PATH`): the text for the agent, empty when it has nothing to say. */
export interface PaneCheckAnswer {
	text: string;
}

/**
 * The variable that gives a project pane's extension the argv of the role (`selfCommand()`, then `PANE_CHECK_ROLE`) as a
 * JSON array of strings (0617), the way `pushHookEnv` gives git the pre-push hook's command.
 */
export const PANE_CHECK_ENV = "JOCASTA_PANE_CHECK";

/** The extension's file in the shipped config folder, loaded by `-e` in project panes only (0617). */
export const PANE_CHECK_EXTENSION = "pane-check-extension.ts";

/**
 * How long the service holds a pane's request for its check (0617): under the role's limit and omp's 30 s for a
 * handler, so a slower check lets the turn end and is owed to the next one.
 */
export const PANE_CHECK_WAIT_MS = 20_000;

/** How long the role waits for the service: past `PANE_CHECK_WAIT_MS` and the tree key, under the extension's limit. */
export const PANE_CHECK_ASK_MS = 25_000;

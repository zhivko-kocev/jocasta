/**
 * The variable that carries a run's boss key into the boss pane's environment, and only there (0484): the service sets
 * it, and `jocasta mcp` in that pane sends it with each tool call.
 */
export const BOSS_KEY_ENV = "JOCASTA_BOSS_KEY";

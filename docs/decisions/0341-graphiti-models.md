# 0341 Graphiti's models

- Status: decided
- Kind: product
- Phase/slice: 5/5.9
- Date: 2026-10-08

## Question
Which models does Graphiti's extraction use through Jarvis's endpoint (0334)?

## Facts
Verified 2026-10-08 in Graphiti's README (https://github.com/getzep/graphiti):
- Graphiti depends on structured JSON output for entity and edge extraction and deduplication; it works best on capable models from OpenAI, Anthropic and Gemini, and very small models often emit JSON that doesn't match the schema.
- Its configuration has a main model and a small model for simpler prompts; its Anthropic example routes between `claude-sonnet-4-5` and `claude-haiku-4-5`.
- As built, extraction used the session's model (his default, `claude-opus-5-5`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Sonnet main, Haiku small (Graphiti's pattern) | Graphiti's own guidance; cheaper than Opus | Two model ids to keep current |
| Sonnet for everything | Simpler | More cost on simple prompts |
| The session's model | Best quality | Highest cost |

## Recommendation
Sonnet main, Haiku small.

## Choice
Now (0377): the newest Sonnet and Haiku with auth in Pi's Anthropic list, else in GitHub Copilot's; dotted ids are read as versions.

Originally: Sonnet main, Haiku small: the newest Sonnet and Haiku in Pi's Anthropic model list, moving onto 6.2's model roles when they exist.

## Reason
"for the extraction model we need to see what are the recomendatoons from the graphiti repo or docs"; then chosen.

## Consequences
- Graphiti's service names a main and a small model; Jarvis's endpoint maps them to the Sonnet and Haiku ids.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
- Amended by 0390: the companion serves `/llm/v1` through omp's model registry and logins.

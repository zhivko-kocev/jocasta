# 0637 clap parses the `jocasta` command's words

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
What parses the `jocasta` command's words in the Rust build?

## Facts
Verified 2026-11-15.
- **Today,** `src/shared/route.ts` (97 lines) routes the words, and `entry.ts` prints usage and errors. The builders wrote those texts to carry out the command's records.
- **Size** (on top of the port's base):
  - clap 4.6, trimmed as herdr trims it, adds 4 crates;
  - its binary overhead is 377–574 KiB (rosetta-rs benchmark);
  - lexopt adds 1 crate and 37 KiB.
- **clap's output.** clap generates help and errors in its own format, and can generate shell completions.

## Options
| Option | For | Against |
|---|---|---|
| Our own routing, ported as it is | the texts stay byte for byte | no generated help or completions |
| clap, trimmed | generated help and errors; completions possible | texts take clap's format unless each is overridden |
| lexopt | a correct lexer | little gain over 97 lines |

## Recommendation
Our own routing.

## Choice
clap 4.6 parses the words.

## Reason
His answer: `clap`.

## Consequences
- This differs from the recommendation.
- **What changes:** help and usage-error texts take clap's format.
- **What stays as decided:**
  - the words and flags;
  - the internal roles, kept out of the help;
  - the exit codes in plan appendix I.

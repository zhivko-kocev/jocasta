# 0590 Models per role after Fable 5.2: the boss on Fable, heavy threads on his slow role

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Vision: V8, V11, V13
- Date: 2026-11-10

## Question
Anthropic released Claude Fable 5.2 on 2026-11-09 and said which model fits which work. Which model and effort does each of his omp roles get, which model does the boss run on, how does a thread get the strongest model when its work needs it, and do the benchmark arms change?

## Facts
Verified 2026-11-10.
- Anthropic's post (https://www.anthropic.com/claude-fable-and-mythos-5-2, 2026-11-09):
  - Fable 5.2 is Mythos 5.2 for every user. It costs $8/$40 per M tokens and is 30% faster than Fable 5.1. Its safeguards hand blocked security work (pentests, exploits) to Opus 4.8.
  - Anthropic's guidance: start most coding and knowledge work with Opus 5.5, "our strongest daily driver"; go up to Fable for the hardest work, and down to Sonnet or Haiku 5.5. Fable can serve as advisor or orchestrator.
  - Benchmarks:
    - FrontierCode 2.0: Fable at high 45.3%, against Opus 5.5's best of 45.5%, for about half the cost. At xhigh 49.1%, the best of any released model.
    - FrontierSWE v2: 69.9%, against Opus 5.5's 62.3%.
    - WANDR: Fable at medium matches Opus 5.5's best for about 40% of the cost.
    - Humanity's Last Exam: Fable at low matches it for under a tenth of the cost.
    - Chartography: Fable at medium 66.4%, against Opus 5.5 at max 64.4%, for under a tenth of the cost.
  - Medium is the default effort in Claude's apps and Claude Code. On Pro plans and standard Team seats, Fable is capped at 50% of the weekly limits.
- Prices per M tokens in omp's catalog (input/output): Fable 5.2 $8/$40 (1M context), Opus 5.5 $4/$20, Sonnet 5.5 $2/$10, Haiku 5.5 $0.10/$0.50, Haiku 4.5 $1/$5.
- His roles before (`~/.omp/agent/config.yml`):
  - `default` Opus 5.5 high; `plan` and `slow` Opus 5.5 max; `task` Opus 5.5 medium; `advisor` Sonnet 5.5 xhigh; `memory` Sonnet 5.5 medium; `vision` Opus 5.5 low.
  - `smol` and `commit` Haiku 4.5 low; `judge` Haiku 4.5; `tiny` a local model with Haiku 4.5 as its fallback.
  - The `plan` and `slow` fallbacks: Fable 5.1 max, then Opus 5 max.
- Jocasta picks models in three places only: `config/boss.yml`'s `modelRoles` (`web` and `dictation`), the eval overlays, and the benchmark arms (0543: a0, a3, a4, a6 and a7 pin Opus 5.5 at high). Everything else follows his roles:
  - project and thread panes run `default`, plan mode runs `plan`, and every thread starts in plan mode;
  - the reviewer (0488) and the drift review run on `slow`;
  - omp's advisor reads every turn;
  - mnemopi's notes, in every Jocasta pane and the boss, run on his `memory` role: the overlays' `mnemopi.llmMode: smol` resolves `memory`, not `smol` (omp `mnemopi/backend.ts:559`);
  - Graphiti's main and small models follow `default` and `smol` (0463).
- omp 18.18.6 (0589):
  - The plan review's "continue with" slider picks which role's model carries out an approved plan, and it always starts on the `default` tier, "no matter which model drove the planning conversation" (`modes/interactive-mode.ts:7853-7861`). A thread started with `--model @slow` would therefore plan on `plan` and carry out its plan on `default`, unless he moved the slider.
  - A role's value may name another role: `modelRoles.default: "@slow"` resolves to his `slow` role, with a cycle guard (`config/model-resolver.ts:1215-1247`, `config/model-roles.ts:19`).

## Options
The questions, one per role (2026-11-10):

| Question | Options | Recommended |
|---|---|---|
| The boss's model | Fable 5.2 at medium; Fable 5.2 at high; keep his `default` (Opus 5.5 high) | Fable 5.2 at medium |
| The boss's effort, asked again | High; Xhigh | High |
| Threads' coding (`default`) | Keep Opus 5.5 high; Fable 5.2 high; Fable 5.2 medium | Keep Opus 5.5 high |
| Planning (`plan`) | Fable 5.2 xhigh; Fable 5.2 high; keep Opus 5.5 max | Fable 5.2 xhigh |
| Reviews and deep thinking (`slow`) | Fable 5.2 high; Fable 5.2 xhigh; keep Opus 5.5 max | Fable 5.2 high |
| The advisor (`advisor`) | Fable 5.2 low; keep Sonnet 5.5 xhigh; Fable 5.2 medium | Fable 5.2 low |
| Small jobs (`smol`, `commit`, `judge`, `tiny`'s fallback) | Haiku 5.5; keep Haiku 4.5; Sonnet 5.5 low | Haiku 5.5 |
| Images (`vision`) | Fable 5.2 medium; keep Opus 5.5 low; Fable 5.2 low | Fable 5.2 medium |
| Who moves a heavy thread to Fable | The boss decides per thread; he switches by hand in the pane; no switching | The boss decides per thread |
| The benchmark arms | Keep Opus 5.5 high; Fable 5.2 high in every arm; add a Fable 5.2 arm | Keep Opus 5.5 high |

Pros and cons as asked, for the choices that weigh most:

| Option | Pros | Cons |
|---|---|---|
| The boss on Fable 5.2 at high | FrontierCode: about Opus 5.5's best for about half its cost; the blog names orchestration as a Fable use | Twice Opus's price per token on every boss turn; needs omp 18.18.5 or later |
| The boss at xhigh | The best judgment per decision | Every routine turn (a relayed question, small talk) thinks longest: slower replies, the most Fable allowance |
| The boss decides heavy threads | The blog's orchestrator pattern; no extra step for him; no model id in Jocasta | A new `thread_start` option and CLI flag; the boss can misjudge; a heavy thread spends Fable allowance fastest |
| Keep the benchmark on Opus 5.5 high | As pre-registered; the blog's daily driver; A7's key and the pinned price table cover it | The harnesses are measured on last week's top model |

## Recommendation
As in the table. For the boss, medium at first; asked again between high and xhigh, high: the boss hands the heavy work to threads, so xhigh's gain would mostly land on work it doesn't do.

## Choice
- **His omp roles** (`~/.omp/agent/config.yml`, outside the repo):

| Role | Model and effort |
|---|---|
| `default` | Opus 5.5 at xhigh (`anthropic/claude-opus-5-5:xhigh`) |
| `plan` | Fable 5.2 at xhigh |
| `slow` | Fable 5.2 at xhigh |
| `advisor` | Fable 5.2 at medium |
| `smol` and `commit` | Haiku 5.5 at low |
| `judge` | Haiku 5.5 |
| `tiny` | unchanged, with Haiku 5.5 as its fallback |
| `vision` | Fable 5.2 at medium, higher later |

  `task` and `memory` are decided in 0591.
- **The boss** runs on Fable 5.2 at high: `config/boss.yml` sets `modelRoles.default: anthropic/claude-fable-5-2:high`.
- **Heavy threads:** the boss decides per thread.
  - `thread_start` takes `heavy` (true or false), and `jocasta thread new … --heavy` does the same. The boss manual tells the boss to start a thread heavy when its work is ambitious or open-ended: a migration, a deep bug, a large refactor.
  - A heavy thread runs on his `slow` role (Fable 5.2 at xhigh) in place of `default`, from its plan through its execution, and across restarts.
- **The benchmark arms** stay on Opus 5.5 at high, as pre-registered (0543 unchanged).

## Reason
- The boss: "lets go role by role because i think that the boss can be fable now in the blog it says it can be used for orchestration". Then "i am thinking between high and xhigh for orchestration", and "okay than high but xhigh for heavy coding i guess".
- `default`: "the default can be opus 5 5 yes with xhigh i think because the quality may be fall down but for very heavy tasks they can switch to fable".
- `plan`, `slow`, `advisor`, the small jobs, heavy threads and the benchmark arms were his picks from the options. For `vision`: "the recomended and we will swithc to higher effort in the future".

## Consequences
- **How a heavy thread runs:** the question named omp's `--model @slow`, but omp 18.18.6's slider would carry out a heavy thread's approved plan on `default`. A heavy thread's omp therefore gets one more `--config`: the shipped `config/heavy.yml`, which sets `modelRoles.default: "@slow"`. In that pane his `default` is his `slow`: the slider starts there and the plan runs there. Jocasta still names no model.
  - The thread's task record (`thread.json`) keeps `heavy`, so a restarted or restored pane starts heavy again.
  - `thread_start`'s reply says so.
- **Fallbacks:**
  - The boss falls back to Opus 5.5 at high, its model before, then Opus 5 at high: `config/boss.yml` sets `retry.fallbackChains.default`, the only chain it overrides.
  - In his config, `plan` and `slow` fall back to Opus 5.5 at max, then Opus 5 at max, in place of Fable 5.1. `vision` falls back to Opus 5.5 at low first.
- Reviews stay on Anthropic (0488); the reviewer moves to Fable 5.2 with his `slow` role. A security-flavoured review can be answered by Opus 4.8 under Fable's safeguards.
- Fable parts need omp 18.18.5 or later; Jocasta moves to 18.18.6 first (0589).
- Plan appendix I lists `thread_start`'s `heavy`, `jocasta thread new --heavy` and `config/heavy.yml`. 0424's `boss.yml` gains a model and its fallback; 0544's `thread new` gains `--heavy`.

# 0614 Quality and security tools: what enters the flow, what stays on demand

- Status: v1
- Kind: product, process
- Phase/slice: program
- Date: 2026-11-14

## Question
In 0610 he asked for mutation testing and the duplicate-code and dead-code tools to be explained before deciding, and named eleven tools of his for AI-generated code: Strix, Cloudflare Security Audit, Trivy, Nuclei, NVIDIA SkillSpector, Semgrep, OpenAI Codex Security, ClamAV, Claude Code's security review, BLA and Microsoft PyRIT. Which enter Jocasta's flow, and where?

## Facts
Verified 2026-11-14 from each tool's own repository, docs or paper (report `sota-quality-tools.md` in `D:/jocasta-spikes/sota/reports/`, outside the repo). No tool was installed or run for the report; run times are estimates.
- **Mutation testing** changes one small thing in the code, runs the tests, and counts the changes no test notices. Coding agents' own test suites scored under 60% on it (TestSuiteBench, abstract read). StrykerJS 10.1.1 crashes at startup with TypeScript 7 (its fix is an open pull request); its Bun runner needs TypeScript below 7. Through the plain command runner each change costs a whole suite run, about 100 s here.
- **jscpd** 5.4.3 (MIT) finds copied blocks by tokens: one binary, seconds, can look at changed files only. It misses a copy rewritten in another style.
- **knip** 6.45.1 (ISC) finds files, exports and dependencies nothing uses. It has a Bun plugin and needs the entry points declared. The repo has 96 unused exports by an earlier count.
- **`bun audit`** ships with Bun. Run on the repo that day: 5 advisories, 1 critical and 1 high, all in `dompurify` 3.4.15, which `monaco-editor` 0.57.0 pins. 3.4.17 fixes them.
- **Package names an agent makes up:** 3.6 to 5.0% of package mentions on 2026 models; about 1% of such names were registered by someone within six months (abstracts read). None of the eleven tools checks for it. Bun's `minimumReleaseAge` refuses packages younger than a set age; whiteboard's own workspace sets 7 days.
- **The eleven, by kind:**
  - **Scan code, no model:** Semgrep (pattern rules; strongest on web services; Windows support not verified; its registry rules may not be shipped inside another product) and Trivy (dependencies and committed secrets; one Windows binary; itself compromised in March 2026, so pinned with a checksum).
  - **Scan code, with a model:** Codex Security, Claude Code's `/security-review`, Cloudflare's security-audit skill, and omp's own `security_scan`, which is on in his config (0593) and runs read-only `security-reviewer` agents on his model. No independent precision or cost figure was found for any of them.
  - **Attack a running app:** Nuclei and Strix. Jocasta has no web app; Strix needs Docker.
  - **Test an AI system:** NVIDIA SkillSpector scans skills and prompts for injection and exfiltration, with a static mode that calls no model. PyRIT attacks a model endpoint. "BLA" is `qybaihe/bla`, which hardens release files against AI reverse engineering and scans nothing.
  - **Malware:** ClamAV matches known malware in files.
- **Jocasta's check runner** already runs a project's mise `verify` task and caches the result by git tree. The boss's `mise-setup` skill writes that task for a project.

## Options
| Question | Options | Recommended |
|---|---|---|
| The dompurify advisories | An override to 3.4.17 now; wait for Monaco | The override |
| `bun audit` | In CI and as a scorecard line; in `mise run check` too; no | CI and the scorecard |
| A minimum package age | 7 days in Jocasta's `bunfig.toml`; none | 7 days |
| knip | In `mise run check`; in the scorecard only; no | In `mise run check` |
| jscpd | A scorecard line; a failing check; no | A scorecard line |
| SkillSpector | Its static mode once, as a spike; a release check now; no | The spike |
| Scanners in his projects | `mise-setup` offers them per project; Jocasta runs them itself; no | `mise-setup` offers them |
| Mutation testing | Now; when Stryker runs with TypeScript 7 | When it runs |
| A security pass in every review | Yes; on demand | On demand |
| Trivy in Jocasta's own repo | Yes; no | No |

## Recommendation
As in the table's last column.

## Choice
- **dompurify** is overridden to 3.4.17 in `package.json`.
- **`bun audit`** runs in CI, failing on high and critical advisories, and is a line of the scorecard (0610). It is not in `mise run check`: it needs the network.
- **Packages must be 7 days old:** `minimumReleaseAge` in Jocasta's `bunfig.toml`.
- **knip** is a dev dependency and runs in `mise run check`. Today's findings are deleted or declared first.
- **jscpd** is a dev dependency and a line of the scorecard: the share of duplicated code, week on week. It fails nothing.
- **SkillSpector's static mode runs once** over Jocasta's shipped skills and prompts, in `D:/jocasta-spikes/`, installed there with `uv tool install` from NVIDIA's repository. Whether it becomes a release check is asked with its result.
- **Its result** (v2.13.4, 2026-11-14; it runs on Windows in 5 to 7 s a folder): 5 findings over everything Jocasta ships, all false positives, two of them rated high (a licence text, the word `.env`, a line about not asking twice, a model alias). It was not tried on a planted bad prompt. **It is not a release check.** It is run on demand when a skill or plugin from someone else is about to be added.
- **The `mise-setup` skill offers scanners per project:** when it writes a project's `verify` task it offers the ones that fit (Trivy for secrets and dependencies; Semgrep for a web service; knip and jscpd for TypeScript), pinned through mise. He says yes or no per project.
- **Not taken:** mutation testing (looked at again when Stryker runs with TypeScript 7; then on demand, one file at a time), a security pass in every thread's review, and Trivy in Jocasta's own repo.
- **Nothing is built for the rest.** omp's `security_scan`, Codex Security, Claude Code's review and Cloudflare's skill are used on demand. Nuclei and Strix fit a web project with a staging address. PyRIT, ClamAV and BLA do not apply now.

## Reason
His ticks. He left the BLA question empty: `qybaihe/bla` is the project he meant. On SkillSpector's result (2026-11-15) he chose `on-demand`.

## Consequences
- Two new dev dependencies, knip and jscpd (0005: approved by his ticks), and one install outside the repo for the spike.
- The age rule also binds those two: a version younger than 7 days is not installed.
- Amends 0472 (the `mise-setup` skill's text) and 0036 (the toolchain).
- The CI workflow gains a step that needs the network, which CI already has for its install.

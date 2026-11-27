# Jocasta decision log

The decision log for Jocasta (named Jarvis until 0480): one record per decision Zhivko made in a grilling session.
The vision image is the source of truth (0019); `docs/plan.md` section 1 transcribes it as V1–V14.
Records are edited in place, and git keeps the history (0014).
This index is also the feature register (0016); requests after v1 are GitHub issues that link the record (0018).

| # | Decision | Kind | Status | Phase/slice | Date | Amended by |
|---|---|---|---|---|---|---|
| [0001](0001-slice-cadence.md) | Slice cadence | process | decided | planning | 2026-10-02 |  |
| [0002](0002-records-before-code.md) | Records before code | process | decided | planning | 2026-10-02 |  |
| [0003](0003-phase-end-review.md) | Phase-end review | process | decided | planning | 2026-10-02 | 0312, 0564 |
| [0004](0004-reopen-planning-decisions.md) | Re-open planning decisions | process | decided | planning | 2026-10-02 |  |
| [0005](0005-what-counts-as-a-decision.md) | What counts as a decision | process | decided | planning | 2026-10-02 | 0618 |
| [0006](0006-later-phase-designs.md) | Later-phase designs | process | decided | planning | 2026-10-02 |  |
| [0007](0007-scope.md) | Scope | process | decided | planning | 2026-10-02 |  |
| [0008](0008-features-in-their-slice.md) | Features in their slice | process | decided | planning | 2026-10-02 |  |
| [0009](0009-harness-base.md) | Harness base | product | decided | planning | 2026-10-02 |  |
| [0010](0010-plan-scope.md) | Plan scope | product | decided | planning | 2026-10-03 |  |
| [0011](0011-phase-map.md) | Phase map | product | decided | planning | 2026-10-03 |  |
| [0012](0012-repo.md) | Repo | product | decided | planning | 2026-10-02 |  |
| [0013](0013-record-shape.md) | Record shape | process | decided | planning | 2026-10-02 |  |
| [0014](0014-changed-decisions.md) | Changed decisions | process | decided | planning | 2026-10-02 |  |
| [0015](0015-file-naming.md) | File naming | process | decided | planning | 2026-10-02 |  |
| [0016](0016-feature-register.md) | Feature register | process | decided | planning | 2026-10-02 |  |
| [0017](0017-first-records.md) | First records | process | decided | planning | 2026-10-02 |  |
| [0018](0018-requests-after-v1.md) | Requests after v1 | process | decided | planning | 2026-10-02 |  |
| [0019](0019-source-of-truth.md) | Source of truth | process | decided | planning | 2026-10-03 |  |
| [0020](0020-grilling-sessions.md) | Grilling sessions | process | decided | planning | 2026-10-03 |  |
| [0021](0021-frontier.md) | Frontier | product | decided | planning | 2026-10-03 |  |
| [0022](0022-no-sessions.md) | No sessions | product | decided | planning | 2026-10-03 |  |
| [0023](0023-feature-catalog.md) | Feature catalog | process | decided | planning | 2026-10-03 |  |
| [0024](0024-harness-name.md) | Harness name | product | decided | 0/0.1 | 2026-10-03 |  |
| [0025](0025-principles.md) | Principles | process | decided | planning | 2026-10-03 |  |
| [0026](0026-fixture-gate.md) | Fixture gate | process | decided | planning | 2026-10-03 |  |
| [0027](0027-seams-and-registries.md) | Seams and registries | process | decided | planning | 2026-10-03 |  |
| [0028](0028-older-plan-material.md) | Older-plan material | process | decided | planning | 2026-10-03 |  |
| [0029](0029-jarvis-features.md) | Jarvis features | product | decided | planning | 2026-10-03 |  |
| [0030](0030-resources-in-containers.md) | Resources in containers | product | decided | planning | 2026-10-03 |  |
| [0031](0031-record-template-and-index.md) | Record template and index | process | decided | 0/0.0 | 2026-10-04 | 0622 |
| [0032](0032-non-goals.md) | Non-goals for v1 | process | decided | 0/0.0 | 2026-10-04 |  |
| [0033](0033-one-binary-two-roles.md) | One binary with two roles | product | decided | 0/0.1 | 2026-10-04 |  |
| [0034](0034-bare-jarvis-until-phase-4.md) | Bare `jarvis` until phase 4 | product | decided | 0/0.1 | 2026-10-04 |  |
| [0035](0035-repo-visibility.md) | Repo visibility | product | decided | 0/0.1 | 2026-10-04 |  |
| [0036](0036-toolchain.md) | Toolchain | product | decided | 0/0.1 | 2026-10-04 | 0610, 0614, 0625 |
| [0037](0037-typescript-settings.md) | TypeScript settings | product | decided | 0/0.1 | 2026-10-04 | 0610, 0625 |
| [0038](0038-repo-layout.md) | Repo layout | product | decided | 0/0.1 | 2026-10-04 | 0629 |
| [0039](0039-repo-docs.md) | Repo docs | product | decided | 0/0.1 | 2026-10-04 | 0646 |
| [0040](0040-harness-entry.md) | Harness entry | product | decided | 0/0.2 | 2026-10-04 |  |
| [0041](0041-pi-version-pin.md) | Pi version pin | product | decided | 0/0.2 | 2026-10-04 |  |
| [0042](0042-bun-pin.md) | Bun pin | product | decided | 0/0.2 | 2026-10-04 | 0435 |
| [0043](0043-build-targets.md) | Build targets in v1 | product | decided | 0/0.2 | 2026-10-04 |  |
| [0044](0044-packaging.md) | Packaging | product | decided | 0/0.2 | 2026-10-04 |  |
| [0045](0045-pi-packages-and-reload.md) | Pi packages and /reload | feature | v1 | 0/0.2 | 2026-10-04 |  |
| [0046](0046-builtin-mcp-codemode-toolsearch.md) | Built-in MCP, Codemode and tool_search | feature | v1 | 0/0.2 | 2026-10-04 |  |
| [0047](0047-share-command.md) | /share to a GitHub gist | feature | v1 | 0/0.2 | 2026-10-04 |  |
| [0048](0048-telemetry.md) | Telemetry | product | decided | 0/0.2 | 2026-10-04 |  |
| [0049](0049-install-scripts.md) | Install scripts | product | decided | 0/0.2 | 2026-10-04 | 0579, 0691, 0692 |
| [0050](0050-self-upgrade.md) | Self-upgrade command | feature | after v1 | 0/0.2 | 2026-10-04 |  |
| [0051](0051-releases.md) | Releases | product | decided | 0/0.2 | 2026-10-04 | 0691 |
| [0052](0052-version-output.md) | --version output | product | decided | 0/0.2 | 2026-10-04 |  |
| [0053](0053-bun-bootstrap-import.md) | Bun bootstrap import | product | decided | 0/0.2 | 2026-10-04 |  |
| [0054](0054-config-dir.md) | Config dir | product | decided | 0/0.3 | 2026-10-04 |  |
| [0055](0055-config-dir-contents.md) | Config dir contents and owners | product | decided | 0/0.3 | 2026-10-04 |  |
| [0056](0056-default-settings.md) | Default settings | product | decided | 0/0.3 | 2026-10-04 | 0377 |
| [0057](0057-phase-0-history.md) | Where history lives in phase 0 | product | decided | 0/0.3 | 2026-10-04 |  |
| [0058](0058-telemetry-mechanism.md) | Telemetry off: mechanism | product | decided | 0/0.3 | 2026-10-04 |  |
| [0059](0059-managed-tools.md) | fd and ripgrep downloads | product | decided | 0/0.3 | 2026-10-04 |  |
| [0060](0060-startup-header.md) | Startup header | product | decided | 0/0.3 | 2026-10-04 | 0378 |
| [0061](0061-pi-update-check.md) | Pi's update check | product | decided | 0/0.3 | 2026-10-04 |  |
| [0062](0062-anthropic-route.md) | Anthropic sign-in route | product | decided | 0/0.4 | 2026-10-04 | 0377 |
| [0063](0063-credential-storage.md) | Credential storage | product | decided | 0/0.4 | 2026-10-04 |  |
| [0064](0064-providers.md) | Providers | product | decided | 0/0.4 | 2026-10-04 |  |
| [0065](0065-day-one-guard.md) | Day-one guard | product | decided | 0/0.5 | 2026-10-04 |  |
| [0066](0066-identity.md) | Identity | product | decided | 0/0.6 | 2026-10-04 |  |
| [0067](0067-rules.md) | Rules | product | decided | 0/0.6 | 2026-10-04 |  |
| [0068](0068-persona.md) | Persona | product | decided | 0/0.6 | 2026-10-04 |  |
| [0069](0069-persona-location.md) | Where identity and persona live | product | decided | 0/0.6 | 2026-10-04 |  |
| [0070](0070-container-scope.md) | What runs in containers | product | decided | 0/0.7 | 2026-10-04 | 0345 |
| [0071](0071-container-runtime.md) | Container runtime | product | decided | 0/0.7 | 2026-10-04 |  |
| [0072](0072-container-management.md) | Container management | product | decided | 0/0.7 | 2026-10-04 |  |
| [0073](0073-container-lifecycle.md) | When containers run | product | decided | 0/0.7 | 2026-10-04 |  |
| [0074](0074-container-images.md) | Container images | product | decided | 0/0.7 | 2026-10-04 |  |
| [0075](0075-container-manager-timing.md) | When the container manager is built | process | decided | 0/0.7 | 2026-10-04 |  |
| [0076](0076-phase-0-smoke.md) | Phase 0 smoke | process | decided | 0/0.8 | 2026-10-04 |  |
| [0077](0077-catalog-additions-harness-comparison.md) | Catalog additions from the harness comparison | product | decided | 0/appendix A | 2026-10-07 |  |
| [0078](0078-research-list-fts5-and-tool-result-failures.md) | Research list: FTS5 in a compiled binary, failing `tool_result` handlers | process | decided | 0/5.4 | 2026-10-07 |  |
| [0079](0079-evals-and-hillclimbing.md) | Evals and hillclimbing | product | decided | 0/1.10 | 2026-10-07 |  |
| [0080](0080-run-budgets.md) | Run budgets | product | decided | 0/appendix G | 2026-10-07 |  |
| [0081](0081-catalog-rows-claude-code-mods.md) | Catalog rows from Claude Code mods | product | decided | 0/appendix A | 2026-10-07 |  |
| [0082](0082-catalog-rows-strands-agents.md) | Catalog rows from Strands Agents | product | decided | 0/appendix A | 2026-10-07 |  |
| [0083](0083-loop-guard-and-nested-agents-md-rows.md) | Loop-guard and nested AGENTS.md rows | product | decided | 0/appendix A | 2026-10-07 |  |
| [0084](0084-research-answers-tool-results-evals-faux-provider.md) | Research answers: tool results in the session file, Pi's evals, Pi's faux provider | process | decided | 0/5.4 | 2026-10-07 |  |
| [0085](0085-learning-repo-as-a-source.md) | The learning repo as a source | process | decided | 0/section 4 | 2026-10-07 |  |
| [0086](0086-grilling-program.md) | Grilling program | process | decided | 0/section 4 | 2026-10-07 |  |
| [0087](0087-principles-minimal-and-standalone.md) | Principles: minimal by default, the harness stands alone | process | decided | 0/section 4 | 2026-10-07 |  |
| [0088](0088-older-plans-decided-fresh.md) | Older plans: decided fresh | process | decided | 0/section 4 | 2026-10-07 |  |
| [0089](0089-question-batching.md) | Question batching | process | decided | 0/section 4 | 2026-10-07 |  |
| [0090](0090-slice-1-1-agenda.md) | Slice 1.1 agenda | process | decided | 1/1.1 | 2026-10-07 |  |
| [0091](0091-slice-1-2-agenda.md) | Slice 1.2 agenda | process | decided | 1/1.2 | 2026-10-07 |  |
| [0092](0092-slice-1-3-agenda.md) | Slice 1.3 agenda | process | decided | 1/1.3 | 2026-10-07 | 0262 |
| [0093](0093-slice-1-4-agenda.md) | Slice 1.4 agenda | process | decided | 1/1.4 | 2026-10-07 |  |
| [0094](0094-client-naming-config-location.md) | Where client-naming config lives | product | decided | 1/1.1 | 2026-10-07 |  |
| [0095](0095-data-dir.md) | Data dir | product | decided | 1/1.1 | 2026-10-07 |  |
| [0096](0096-db-files.md) | DB files | product | decided | 1/1.1 | 2026-10-07 |  |
| [0097](0097-sqlite-driver.md) | SQLite driver | product | decided | 1/1.1 | 2026-10-07 |  |
| [0098](0098-dir-overrides.md) | Dir overrides | product | decided | 1/1.1 | 2026-10-07 |  |
| [0099](0099-sqlite-connection-settings.md) | SQLite connection settings | product | decided | 1/1.1 | 2026-10-07 |  |
| [0100](0100-sqlite-schema-rules.md) | SQLite schema rules | product | decided | 1/1.1 | 2026-10-07 |  |
| [0101](0101-db-failures.md) | DB failures | product | decided | 1/1.1 | 2026-10-07 |  |
| [0102](0102-project-source.md) | Where projects come from | product | decided | 1/1.2 | 2026-10-07 |  |
| [0103](0103-project-identity.md) | Project identity | product | decided | 1/1.2 | 2026-10-07 |  |
| [0104](0104-clones-and-worktrees.md) | Clones and worktrees | product | decided | 1/1.2 | 2026-10-07 |  |
| [0105](0105-what-counts-as-a-project.md) | What counts as a project | product | decided | 1/1.2 | 2026-10-07 |  |
| [0106](0106-never-projects.md) | Never-projects | product | decided | 1/1.2 | 2026-10-07 |  |
| [0107](0107-project-ids.md) | Project ids | product | decided | 1/1.2 | 2026-10-07 |  |
| [0108](0108-root-scan-timing.md) | When roots are scanned | product | decided | 1/1.2 | 2026-10-07 |  |
| [0109](0109-projects-file.md) | The projects file | product | decided | 1/1.2 | 2026-10-07 | 0262, 0613 |
| [0110](0110-project-display-name.md) | Project display name | product | decided | 1/1.2 | 2026-10-07 |  |
| [0111](0111-project-commands.md) | `project` commands | product | decided | 1/1.2 | 2026-10-07 |  |
| [0112](0112-project-rename-entry.md) | Project rename entry | product | decided | 1/1.2 | 2026-10-07 |  |
| [0113](0113-scopes-status.md) | Scopes status | product | decided | 1/1.3 | 2026-10-07 |  |
| [0114](0114-thread-hierarchy.md) | Thread hierarchy | product | decided | 1/1.4 | 2026-10-07 |  |
| [0115](0115-thread-ids.md) | Thread ids | product | decided | 1/1.4 | 2026-10-07 |  |
| [0116](0116-dormant-label.md) | Dormant label | product | dropped | 1/1.4 | 2026-10-07 | 0307 |
| [0117](0117-trunk-rule.md) | The trunk rule | product | decided | 1/1.4 | 2026-10-07 |  |
| [0118](0118-workspace-checkout.md) | Workspace checkout | product | decided | 1/1.4 | 2026-10-07 |  |
| [0119](0119-workspace-thread-closing.md) | Closing a workspace thread | product | decided | 1/1.4 | 2026-10-07 |  |
| [0120](0120-thread-names.md) | Thread names | product | decided | 1/1.4 | 2026-10-07 |  |
| [0121](0121-workspace-branch-names.md) | Workspace branch names | product | decided | 1/1.4 | 2026-10-07 |  |
| [0122](0122-worktree-cleanup.md) | Worktree and branch cleanup | product | decided | 1/1.4 | 2026-10-07 |  |
| [0123](0123-creating-workspace-threads.md) | Creating workspace threads | product | decided | 1/1.4 | 2026-10-07 |  |
| [0124](0124-detached-head-and-non-git.md) | Detached HEAD and non-git projects | product | decided | 1/1.4 | 2026-10-07 |  |
| [0125](0125-home-thread.md) | The home thread | product | decided | 1/1.4 | 2026-10-07 |  |
| [0126](0126-threads-schema.md) | The threads schema | product | decided | 1/1.4 | 2026-10-07 |  |
| [0127](0127-trunk-set.md) | The trunk set | product | decided | 1/1.4 | 2026-10-07 |  |
| [0128](0128-branch-rename.md) | Branch renamed | product | dropped | 1/1.4 | 2026-10-07 | 0307 |
| [0129](0129-opening-continues.md) | Opening continues | product | decided | 1/1.5 | 2026-10-07 |  |
| [0130](0130-branch-without-thread.md) | A branch with no thread | product | decided | 1/1.5 | 2026-10-07 |  |
| [0131](0131-thread-session-storage.md) | Thread session storage | product | decided | 1/1.5 | 2026-10-07 |  |
| [0132](0132-pi-session-commands.md) | Pi's session commands | product | dropped | 1/1.5 | 2026-10-07 | 0307 |
| [0133](0133-command-dispatch.md) | How Jarvis's commands reach the CLI | product | decided | 1/1.6 | 2026-10-07 |  |
| [0134](0134-lazy-subcommands.md) | Lazy-loaded subcommands | product | decided | 1/1.6 | 2026-10-07 |  |
| [0135](0135-harness-subcommand.md) | Opening the harness alone | product | decided | 1/1.6 | 2026-10-07 |  |
| [0136](0136-past-windows.md) | Past windows | product | decided | 1/1.5 | 2026-10-07 |  |
| [0137](0137-never-shadow-pi.md) | Jarvis never shadows Pi's commands | product | decided | 1/1.6 | 2026-10-07 |  |
| [0138](0138-headless-branch-without-thread.md) | Headless run on a branch with no thread | product | decided | 1/1.7 | 2026-10-07 |  |
| [0139](0139-headless-flags.md) | Headless flags | product | decided | 1/1.7 | 2026-10-07 |  |
| [0140](0140-exit-codes.md) | Exit codes | product | decided | 1/1.7 | 2026-10-07 |  |
| [0141](0141-resumable-headless-runs.md) | Resumable headless runs | product | decided | 1/1.7 | 2026-10-07 |  |
| [0142](0142-slice-1-8-agenda.md) | Slice 1.8 agenda | process | decided | 1/1.8 | 2026-10-07 |  |
| [0143](0143-slice-1-9-agenda.md) | Slice 1.9 agenda | process | decided | 1/1.9 | 2026-10-07 |  |
| [0144](0144-slice-1-11-agenda.md) | Slice 1.11 agenda | process | decided | 1/1.11 | 2026-10-07 |  |
| [0145](0145-slice-1-12-agenda.md) | Slice 1.12 agenda | process | decided | 1/1.12 | 2026-10-07 |  |
| [0146](0146-edit-style.md) | Edit style | product | decided | 1/1.8 | 2026-10-07 |  |
| [0147](0147-tool-override-mechanism.md) | Tool override mechanism | product | decided | 1/1.8 | 2026-10-07 |  |
| [0148](0148-edit-requires-read.md) | Edit-requires-read | product | dropped | 1/1.8 | 2026-10-07 | 0307 |
| [0149](0149-bash-interceptor.md) | Bash interceptor | product | decided | 1/1.8 | 2026-10-07 |  |
| [0150](0150-spill.md) | Spill large output | product | decided | 1/1.8 | 2026-10-07 |  |
| [0151](0151-local-scratch.md) | Thread-local scratch files | product | decided | 1/1.8 | 2026-10-07 |  |
| [0152](0152-loop-guard.md) | Loop guard | product | decided | 1/1.8 | 2026-10-07 |  |
| [0153](0153-core-tool-limits.md) | Core tool limits | product | decided | 1/1.8 | 2026-10-07 |  |
| [0154](0154-claude-project-assets.md) | Claude-format project assets | product | dropped | 1/1.9 | 2026-10-07 |  |
| [0155](0155-agents-md-and-claude-md.md) | AGENTS.md beside CLAUDE.md | product | decided | 1/1.9 | 2026-10-07 |  |
| [0156](0156-rules-on-read.md) | Path-scoped rules and nested AGENTS.md on read | product | decided | 1/1.9 | 2026-10-07 | 0232 |
| [0157](0157-user-level-claude.md) | User-level `~/.claude` | product | decided | 1/1.9 | 2026-10-07 |  |
| [0158](0158-claude-asset-kinds.md) | Which Claude-format kinds load | product | dropped | 1/1.9 | 2026-10-07 |  |
| [0159](0159-context-imports.md) | `@path` imports | product | dropped | 1/1.9 | 2026-10-07 |  |
| [0160](0160-command-arguments.md) | Command arguments | product | dropped | 1/1.9 | 2026-10-07 |  |
| [0161](0161-fake-provider.md) | Fake provider | product | decided | 1/1.10 | 2026-10-07 | 0376 |
| [0162](0162-fake-script-format.md) | Fake script format | product | decided | 1/1.10 | 2026-10-07 | 0376 |
| [0163](0163-eval-cases.md) | Eval cases | product | decided | 1/1.10 | 2026-10-07 |  |
| [0164](0164-eval-graders.md) | Eval graders | product | decided | 1/1.10 | 2026-10-07 |  |
| [0165](0165-eval-runs-and-results.md) | Eval runs and results | product | decided | 1/1.10 | 2026-10-07 |  |
| [0166](0166-eval-locations.md) | Eval locations | product | decided | 1/1.10 | 2026-10-07 | 0375 |
| [0167](0167-hillclimbing.md) | Hillclimbing | product | decided | 1/1.10 | 2026-10-07 |  |
| [0168](0168-who-climbs.md) | Who runs a climb | product | decided | 1/1.10 | 2026-10-07 | 0376 |
| [0169](0169-when-evals-run.md) | When evals run | product | decided | 1/1.10 | 2026-10-07 | 0376 |
| [0170](0170-cache-warming.md) | Cache warming | product | decided | 1/1.11 | 2026-10-07 |  |
| [0171](0171-pi-self-references.md) | Pi's self-references in the prompt | product | decided | 1/1.11 | 2026-10-07 |  |
| [0172](0172-jarvis-prompt-section.md) | Jarvis's prompt section | product | decided | 1/1.11 | 2026-10-07 | 0314, 0381 |
| [0173](0173-cost-display.md) | Cost display | product | dropped | 1/1.11 | 2026-10-07 | 0307 |
| [0174](0174-context-gauge.md) | Context gauge | product | dropped | 1/1.11 | 2026-10-07 | 0307 |
| [0175](0175-self-extension.md) | Self-extension | product | decided | 1/1.11 | 2026-10-07 |  |
| [0176](0176-sampling-parameters.md) | Sampling parameters | product | decided | 1/1.12 | 2026-10-07 |  |
| [0177](0177-tool-call-format.md) | Tool-call format | product | decided | 1/1.12 | 2026-10-07 |  |
| [0178](0178-drifted-edits.md) | Drifted edits | product | decided | 1/1.12 | 2026-10-07 |  |
| [0179](0179-speculative-execution.md) | Speculative tool execution | product | decided | 1/1.12 | 2026-10-07 |  |
| [0180](0180-workspace-tree.md) | Workspace tree in the prompt | product | decided | 1/1.12 | 2026-10-07 |  |
| [0181](0181-stop-detection.md) | Stop detection | product | decided | 1/1.12 | 2026-10-07 |  |
| [0182](0182-thinking-level.md) | Thinking level | product | decided | 1/1.12 | 2026-10-07 |  |
| [0183](0183-fabricated-tool-results.md) | Fabricated tool results | product | decided | 1/1.12 | 2026-10-07 | 0307 |
| [0184](0184-prompt-cache-retention.md) | Prompt-cache retention | product | decided | 1/1.12 | 2026-10-07 |  |
| [0185](0185-profiles-mechanism.md) | Profiles mechanism | product | decided | 1/1.12 | 2026-10-07 |  |
| [0186](0186-strict-profile.md) | Strict profile | product | decided | 1/1.12 | 2026-10-07 | 0313 |
| [0187](0187-run-budgets-status.md) | Run budgets status | product | decided | 1/1.12 | 2026-10-07 | 0541 |
| [0188](0188-slice-2-1-agenda.md) | Slice 2.1 agenda | process | decided | 2/2.1 | 2026-10-07 |  |
| [0189](0189-slice-2-2-agenda.md) | Slice 2.2 agenda | process | decided | 2/2.2 | 2026-10-07 |  |
| [0190](0190-slice-2-3-agenda.md) | Slice 2.3 agenda | process | decided | 2/2.3 | 2026-10-07 |  |
| [0191](0191-slice-2-4-agenda.md) | Slice 2.4 agenda | process | decided | 2/2.4 | 2026-10-07 |  |
| [0192](0192-pi-pin-1-1-0.md) | Pi pin moves to 1.1.0 now | process | decided | 2/opening | 2026-10-07 |  |
| [0193](0193-slice-2-5-agenda.md) | Slice 2.5 agenda | process | decided | 2/2.5 | 2026-10-08 |  |
| [0194](0194-slice-2-6-agenda.md) | Slice 2.6 agenda | process | decided | 2/2.6 | 2026-10-08 |  |
| [0195](0195-slice-2-7-agenda.md) | Slice 2.7 agenda | process | decided | 2/2.7 | 2026-10-08 |  |
| [0196](0196-policy-file.md) | Policy file | product | decided | 2/2.1 | 2026-10-08 | 0220 |
| [0197](0197-rule-precedence.md) | Rule precedence | product | decided | 2/2.1 | 2026-10-08 |  |
| [0198](0198-unmatched-calls.md) | Calls no rule matches | product | decided | 2/2.1 | 2026-10-08 |  |
| [0199](0199-permission-modes.md) | Permission modes | product | decided | 2/2.1 | 2026-10-08 |  |
| [0200](0200-default-mode.md) | Default mode | product | decided | 2/2.1 | 2026-10-08 |  |
| [0201](0201-critical-patterns.md) | Critical patterns | product | decided | 2/2.1 | 2026-10-08 |  |
| [0202](0202-bang-commands.md) | `!` commands and the guard | product | decided | 2/2.1 | 2026-10-08 |  |
| [0203](0203-shell-parser-spike.md) | Shell parser spike rule | product | decided | 2/2.1 | 2026-10-08 |  |
| [0204](0204-sandbox.md) | Sandbox | product | decided | 2/2.1 | 2026-10-08 |  |
| [0205](0205-history-aware-rules.md) | History-aware rules | product | decided | 2/2.1 | 2026-10-08 | 0307 |
| [0206](0206-broken-policy-file.md) | Broken policy file | product | decided | 2/2.1 | 2026-10-08 |  |
| [0207](0207-asks-in-headless-runs.md) | Asks in headless runs | product | decided | 2/2.1 | 2026-10-08 |  |
| [0208](0208-rule-grammar.md) | Rule grammar | product | decided | 2/2.1 | 2026-10-08 |  |
| [0209](0209-git-guard-implementation.md) | git-guard implementation | product | decided | 2/2.2 | 2026-10-08 |  |
| [0210](0210-git-guard-base-rules.md) | git-guard base rules | product | decided | 2/2.2 | 2026-10-08 | 0218 |
| [0211](0211-guard-denial-text.md) | Guard denial text | product | decided | 2/2.2 | 2026-10-08 |  |
| [0212](0212-guard-corpus-seeds.md) | Guard corpus seeds | product | decided | 2/2.2 | 2026-10-08 |  |
| [0213](0213-recursive-deletes-in-project.md) | Recursive deletes inside the project | product | decided | 2/2.2 | 2026-10-08 |  |
| [0214](0214-sudo-and-git-add.md) | Elevation and `git add` | product | decided | 2/2.2 | 2026-10-08 |  |
| [0215](0215-broken-guard-rules.md) | Broken guard rules or corpus | product | decided | 2/2.2 | 2026-10-08 |  |
| [0216](0216-billion-context-candidate.md) | billion-context as a 3.4 and 3.5 candidate | product | decided | 3/3.4, 3.5 | 2026-10-08 |  |
| [0217](0217-firstmate-reference.md) | firstmate as a phase 4 design reference | product | decided | 4/4.4–4.6 | 2026-10-08 |  |
| [0218](0218-force-with-lease.md) | Force push with lease | product | decided | 2/2.2 | 2026-10-08 |  |
| [0219](0219-trust-mechanism.md) | Project trust mechanism | product | dropped | 2/2.3 | 2026-10-08 | 0307 |
| [0220](0220-trust-gated-resources.md) | What project trust gates | product | decided | 2/2.3 | 2026-10-08 | 0307 |
| [0221](0221-trust-prompt-and-command.md) | Trust prompt and command | product | dropped | 2/2.3 | 2026-10-08 | 0307 |
| [0222](0222-always-grants.md) | Always grants | product | dropped | 2/2.4 | 2026-10-08 | 0307 |
| [0223](0223-dry-run-preview.md) | Dry-run preview in approvals | product | decided | 2/2.4 | 2026-10-08 | 0307 |
| [0224](0224-approval-overlay.md) | Approval overlay | product | decided | 2/2.4 | 2026-10-08 | 0307 |
| [0225](0225-hooks-status.md) | Claude-format hooks status | product | dropped | 2/2.5 | 2026-10-08 |  |
| [0226](0226-hook-events.md) | Hook events | product | dropped | 2/2.5 | 2026-10-08 |  |
| [0227](0227-failing-pre-tool-hooks.md) | Failing pre-tool hooks | product | dropped | 2/2.5 | 2026-10-08 |  |
| [0228](0228-hook-timeouts-and-shell.md) | Hook timeouts and shell | product | dropped | 2/2.5 | 2026-10-08 |  |
| [0229](0229-lean-jarvis-and-pruning-pass.md) | Lean Jarvis and the pruning pass | process | decided | all | 2026-10-08 |  |
| [0230](0230-benchmark-targets.md) | Benchmark targets | product | decided | 1/1.10 | 2026-10-08 |  |
| [0231](0231-no-claude-code-compat-in-v1.md) | No Claude Code compatibility in v1 | product | decided | 1/1.9, 2/2.1, 2.3, 2.5, 6/6.4 | 2026-10-08 |  |
| [0232](0232-nested-agents-md-only.md) | Nested AGENTS.md on read, without path-scoped rules | product | decided | 1/1.9 | 2026-10-08 |  |
| [0233](0233-redaction-status.md) | Secret redaction status | product | decided | 2/2.6 | 2026-10-08 |  |
| [0234](0234-what-is-a-secret.md) | What counts as a secret | product | decided | 2/2.6 | 2026-10-08 |  |
| [0235](0235-where-redaction-runs.md) | Where redaction runs and how it fails | product | decided | 2/2.6 | 2026-10-08 |  |
| [0236](0236-untrusted-content-flag.md) | Untrusted content flag | product | decided | 2/2.6 | 2026-10-08 |  |
| [0237](0237-chat-only-allowed-tools.md) | Chat-only allowed tools | product | decided | 2/2.7 | 2026-10-08 | 0364 |
| [0238](0238-chat-only-shell.md) | Chat-only shell commands | product | decided | 2/2.7 | 2026-10-08 |  |
| [0239](0239-chat-only-hide-or-deny.md) | Chat-only tools declared and denied | product | decided | 2/2.7 | 2026-10-08 |  |
| [0240](0240-chat-only-denial-and-escape.md) | Chat-only denial text and escape hatch | product | decided | 2/2.7 | 2026-10-08 |  |
| [0241](0241-phase-3-slice-status.md) | Phase 3 slice status | product | decided | 3/opening | 2026-10-08 |  |
| [0242](0242-compaction-spike-rule.md) | Smooth compaction spike rule | product | decided | 3/3.4 | 2026-10-08 |  |
| [0243](0243-compaction-defaults.md) | Compaction thresholds, overflow and marker | product | decided | 3/3.4 | 2026-10-08 |  |
| [0244](0244-todo-tool.md) | Todo tool | product | decided | 3/3.2 | 2026-10-08 |  |
| [0245](0245-history-search.md) | History search | product | decided | 3/3.5 | 2026-10-08 |  |
| [0246](0246-ask-tool.md) | Ask tool | product | decided | 3/3.6 | 2026-10-08 |  |
| [0247](0247-report-back.md) | Report-back | product | decided | 3/3.7 | 2026-10-08 | 0361 |
| [0248](0248-done-check-status.md) | Done check status | product | decided | 3/3.7 | 2026-10-08 |  |
| [0249](0249-frontier-own-ui.md) | Frontier is Jarvis's own UI | product | decided | 4/opening | 2026-10-08 |  |
| [0250](0250-thread-runtime.md) | What runs threads behind Frontier | product | decided | 4/4.1, 4.2 | 2026-10-08 | 0357 |
| [0251](0251-frontier-surface.md) | Frontier's UI surface | product | decided | 4/4.1, 4.3 | 2026-10-08 | 0367 |
| [0252](0252-phase-4-slice-status.md) | Phase 4 slice status | product | decided | 4/opening | 2026-10-08 | 0367 |
| [0253](0253-frontier-server-lifetime.md) | Frontier server lifetime | product | decided | 4/4.1 | 2026-10-08 | 0357, 0367, 0434 |
| [0254](0254-frontier-port-and-access.md) | Frontier port, access and single instance | product | decided | 4/4.1 | 2026-10-08 | 0358 |
| [0255](0255-browser-fallback.md) | Browser fallback | product | decided | 4/4.1 | 2026-10-08 | 0366 |
| [0256](0256-threads-on-quit.md) | Running threads when Frontier quits | product | decided | 4/4.1 | 2026-10-08 |  |
| [0257](0257-frontier-layout.md) | Frontier layout | product | decided | 4/4.3 | 2026-10-08 | 0370 |
| [0258](0258-frontier-look.md) | Frontier look and motion | product | decided | 4/4.3 | 2026-10-08 | 0369 |
| [0259](0259-boss-relays-questions.md) | The boss relays threads' questions | product | decided | 4/4.4 | 2026-10-08 |  |
| [0260](0260-boss-shape-and-manual.md) | The boss's shape and manual | product | decided | 4/4.4 | 2026-10-08 |  |
| [0261](0261-boss-tools.md) | The boss's tools | product | decided | 4/4.4 | 2026-10-08 | 0372 |
| [0262](0262-boss-view-and-scopes.md) | What the boss sees, and scopes | product | decided | 4/4.4, 1.3 | 2026-10-08 |  |
| [0263](0263-boss-digest.md) | Boss digest and decisions one at a time | product | decided | 4/4.4 | 2026-10-08 |  |
| [0264](0264-boss-model.md) | The boss's model | product | decided | 4/4.4 | 2026-10-08 |  |
| [0265](0265-dispatch-start.md) | Starting work the boss is asked for | product | decided | 4/4.5 | 2026-10-08 |  |
| [0266](0266-thread-states.md) | Thread runtime states | product | decided | 4/4.5 | 2026-10-08 |  |
| [0267](0267-busy-threads.md) | Work sent to a busy thread | product | decided | 4/4.5 | 2026-10-08 |  |
| [0268](0268-running-thread-limit.md) | How many threads run at once | product | decided | 4/4.5 | 2026-10-08 |  |
| [0269](0269-blocked-flow.md) | Blocked flow | product | decided | 4/4.6 | 2026-10-08 |  |
| [0270](0270-working-by-hand.md) | Working by hand in a boss-run thread | product | decided | 4/4.7 | 2026-10-08 |  |
| [0271](0271-notifications.md) | Notifications | product | decided | 4/4.9 | 2026-10-08 | 0407 |
| [0272](0272-dictation-engine.md) | Dictation engine | product | decided | 4/4.8 | 2026-10-08 | 0330 |
| [0273](0273-dictation-use.md) | Dictation where, mode and submitting | product | decided | 4/4.8 | 2026-10-08 | 0357, 0367, 0368 |
| [0274](0274-hud-contents.md) | HUD contents | product | decided | 4/4.12 | 2026-10-08 | 0362 |
| [0275](0275-usage-warning.md) | Usage warning | product | decided | 4/4.12 | 2026-10-08 |  |
| [0276](0276-workshop-prep.md) | Workshop prep | product | decided | 4/4.13 | 2026-10-08 |  |
| [0277](0277-memory-scope.md) | Where memory lives | product | decided | 5/5.2 | 2026-10-08 |  |
| [0278](0278-memory-layers.md) | Memory layers | product | decided | 5/5.0–5.5 | 2026-10-08 | 0334 |
| [0279](0279-memory-md-writes.md) | Who writes memory.md | product | decided | 5/5.3 | 2026-10-08 |  |
| [0280](0280-boss-memory.md) | The boss's memory | product | decided | 5/5.2, 4.4 | 2026-10-08 | 0342 |
| [0281](0281-phase-5-remaining-status.md) | Phase 5 remaining slices | product | decided | 5/5.3–5.9 | 2026-10-08 |  |
| [0282](0282-phase-6-slice-status.md) | Phase 6 slice status | product | decided | 6/opening | 2026-10-08 |  |
| [0283](0283-model-roles.md) | Model roles and fallbacks | product | decided | 6/6.2 | 2026-10-08 | 0311, 0377 |
| [0284](0284-jobs-wait-send.md) | Jobs, wait and send | product | decided | 6/6.3 | 2026-10-08 |  |
| [0285](0285-plan-mode.md) | Plan mode | product | decided | 6/6.1 | 2026-10-08 |  |
| [0286](0286-subagents.md) | Subagents | product | decided | 6/6.4 | 2026-10-08 |  |
| [0287](0287-advisor-and-stream-rules.md) | Advisor and stream rules | product | decided | 6/6.5 | 2026-10-08 |  |
| [0288](0288-lsp.md) | LSP diagnostics | product | decided | 6/6.6 | 2026-10-08 |  |
| [0289](0289-eval-and-browser.md) | Eval and browser tools | product | decided | 6/6.8 | 2026-10-08 |  |
| [0290](0290-code-and-web-tools.md) | Code and web tools | product | decided | 6/6.9 | 2026-10-08 | 0307, 0347 |
| [0291](0291-phase-7-status.md) | Phase 7 slice status | product | decided | 7/opening | 2026-10-08 |  |
| [0292](0292-animations.md) | Animations | product | decided | 7/7.3 | 2026-10-08 |  |
| [0293](0293-voice-engine.md) | Voice engine | product | decided | 7/7.4 | 2026-10-08 | 0418, 0552 |
| [0294](0294-voice-rules.md) | Voice rules | product | decided | 7/7.4 | 2026-10-08 | 0373, 0552 |
| [0295](0295-drawing.md) | How agents draw | product | decided | 7/7.1–7.2 | 2026-10-08 |  |
| [0296](0296-speak-tool.md) | A speak tool | product | decided | 7/7.4 | 2026-10-08 | 0552, 0583 |
| [0297](0297-phase-8-status.md) | Phase 8 slice status | product | decided | 8/opening | 2026-10-08 |  |
| [0298](0298-event-queue.md) | Event queue and quiet hours | product | decided | 8/8.2 | 2026-10-08 |  |
| [0299](0299-delivery.md) | Delivery | product | decided | 8/8.3 | 2026-10-08 | 0695 |
| [0300](0300-today.md) | The today view | product | decided | 8/8.4 | 2026-10-08 |  |
| [0301](0301-pollers.md) | Pollers | product | decided | 8/8.1 | 2026-10-08 | 0430 |
| [0302](0302-worklog-and-inbox.md) | Worklog and inbox | product | decided | 8/8.5 | 2026-10-08 |  |
| [0303](0303-interjections.md) | Proactive interjections | product | decided | 8/8.7 | 2026-10-08 |  |
| [0304](0304-phase-9-status.md) | Phase 9 status | product | decided | 9/opening | 2026-10-08 |  |
| [0305](0305-hacker-life-areas.md) | Hacker-life areas | product | decided | 8/opening | 2026-10-08 |  |
| [0306](0306-pruning-pass-scope.md) | Pruning pass scope | process | decided | all (0229) | 2026-10-08 |  |
| [0307](0307-harness-prunes.md) | Harness prunes | product | decided | 1.4–6.9 (0229) | 2026-10-08 |  |
| [0308](0308-subagent-effort.md) | Subagent thinking effort | product | decided | 6/6.4 | 2026-10-08 |  |
| [0309](0309-no-voice-command.md) | No voice command | product | decided | 7/7.4 | 2026-10-08 |  |
| [0310](0310-approval-prompt-kept.md) | Approval prompt kept | product | decided | 2/2.4 (0229) | 2026-10-08 |  |
| [0311](0311-in-flight-cap-and-rotation.md) | In-flight cap and account rotation | product | decided | 6/6.2 | 2026-10-08 | 0346, 0377 |
| [0312](0312-build-all-of-v1.md) | Building all of v1 without phase stops | process | decided | all (0003, 0086) | 2026-10-08 |  |
| [0313](0313-strict-profile-file.md) | Strict profile file | product | decided | 1/1.12 | 2026-10-08 | 0377 |
| [0314](0314-self-extension-prompt-line.md) | Self-extension prompt line | product | decided | 1/1.11 | 2026-10-08 |  |
| [0315](0315-branch-override-flag.md) | Branch override flag | product | decided | 1/1.6 | 2026-10-08 |  |
| [0316](0316-phase-1-as-built-texts.md) | Phase 1 as-built texts and tables | product | decided | 1/1.1–1.8 | 2026-10-08 |  |
| [0317](0317-eval-case-format.md) | Eval case format | product | decided | 1/1.10 | 2026-10-08 | 0376 |
| [0318](0318-real-eval-runs.md) | Real eval runs | product | decided | 1/1.10 | 2026-10-08 |  |
| [0319](0319-eval-judge.md) | Model judge for evals | product | decided | 1/1.10 | 2026-10-08 |  |
| [0320](0320-benchmark-adapters.md) | Public benchmark adapters | product | decided | 1/1.10 (0230) | 2026-10-08 |  |
| [0321](0321-permission-mode-switch.md) | Permission mode switch | product | decided | 2/2.1, 2.4 | 2026-10-08 |  |
| [0322](0322-phase-2-as-built.md) | Phase 2 as-built texts | product | decided | 2/2.1–2.6 | 2026-10-08 |  |
| [0323](0323-billion-context-from-start.md) | billion-context from the start | product | decided | 3/3.4, 3.5 | 2026-10-08 | 0325 |
| [0324](0324-omp-shake-and-elision.md) | Shake at task end and useless-result elision | product | decided | 3/3.4 | 2026-10-08 |  |
| [0325](0325-billion-context-proxy-shipping.md) | How billion-context's proxy ships | product | decided | 3/3.4, 0.2 | 2026-10-08 |  |
| [0326](0326-phase-3-as-built.md) | Phase 3 as-built texts | product | decided | 3/3.2–3.7 | 2026-10-08 |  |
| [0327](0327-billion-context-data-location.md) | billion-context's files under the data dir | product | decided | 3/3.4 | 2026-10-08 |  |
| [0328](0328-frontier-ui-stack.md) | Frontier UI stack | product | decided | 4/4.1, 4.3 | 2026-10-08 |  |
| [0329](0329-toasts.md) | In-app toasts and browser notifications | product | decided | 4/4.9 | 2026-10-08 | 0407, 0552 |
| [0330](0330-parakeet-dictation.md) | Parakeet for dictation, no spike | product | decided | 4/4.8 | 2026-10-08 |  |
| [0331](0331-boss-manual-text.md) | The boss manual's text | product | decided | 4/4.4 | 2026-10-08 | 0465 |
| [0332](0332-phase-4-as-built.md) | Phase 4 as-built texts and test scripts | product | decided | 4/4.1–4.12 | 2026-10-08 | 0362, 0376 |
| [0333](0333-phase-4-as-built-2.md) | Phase 4 as-built, second pass | product | decided | 4/4.1–4.12 | 2026-10-08 |  |
| [0334](0334-graphiti-in-v1.md) | Graphiti in v1, through Jarvis | product | decided | 5/5.0, 5.9 | 2026-10-08 | 0430, 0603 |
| [0335](0335-phase-5-build-defaults.md) | Phase 5 build defaults | product | decided | 5/5.1–5.8 | 2026-10-08 |  |
| [0336](0336-anthropic-plan-limits.md) | Plan limits through pi-anthropic-auth | product | decided | 5/5.9 | 2026-10-08 | 0337, 0377 |
| [0337](0337-own-anthropic-oauth.md) | Jarvis's own Anthropic subscription shaping, modeled on omp | product | decided | 5/5.9 | 2026-10-08 | 0377 |
| [0338](0338-subscription-finisher-hop.md) | A finisher hop after billion-context for subscription requests | product | decided | 5/5.9 | 2026-10-08 | 0377 |
| [0339](0339-omp-drift-at-phase-openings.md) | omp drift check at every phase opening | process | decided | 5/5.9 | 2026-10-08 | 0377 |
| [0340](0340-phase-5-as-built.md) | Phase 5 as-built, memory and graph | product | decided | 5/5.1–5.9 | 2026-10-08 |  |
| [0341](0341-graphiti-models.md) | Graphiti's models | product | decided | 5/5.9 | 2026-10-08 | 0377 |
| [0342](0342-personal-graph.md) | A personal graph from confirmed facts and generalized decisions | product | decided | 5/5.2, 5.9 | 2026-10-08 |  |
| [0343](0343-phase-5-as-built-2.md) | Phase 5 as-built, subscription shaping and graph plumbing | product | decided | 5/5.9 | 2026-10-08 | 0377 |
| [0344](0344-phase-5-as-built-3.md) | Phase 5 as-built, last items | product | decided | 5/5.2, 5.5, 5.9 | 2026-10-08 | 0377 |
| [0345](0345-python-browser-on-host.md) | The python and browser tools run on the host | product | decided | 6/6.8 | 2026-10-08 |  |
| [0346](0346-account-rotation-omp-way.md) | Account rotation the omp way | product | decided | 6/6.2 | 2026-10-08 | 0377 |
| [0347](0347-web-search-omp-way.md) | web_search the omp way | product | decided | 6/6.9 | 2026-10-08 | 0364, 0365 |
| [0348](0348-phase-6-as-built.md) | Phase 6 as-built, browser and subagent placement | product | decided | 6/6.1–6.9 | 2026-10-08 | 0364 |
| [0349](0349-cap-and-subagent-approvals.md) | The in-flight cap's mechanics and subagent approvals | product | decided | 6/6.2, 6.4 | 2026-10-08 |  |
| [0350](0350-phase-6-as-built-2.md) | Phase 6 as-built, rotation and search | product | decided | 6/6.2, 6.9 | 2026-10-08 | 0377 |
| [0351](0351-phase-6-as-built-3.md) | Phase 6 as-built, last items | product | decided | 6/6.4, 6.9 | 2026-10-08 |  |
| [0352](0352-canvas-and-voice-libraries.md) | The canvas and voice libraries | product | decided | 7/7.2–7.4 | 2026-10-09 | 0379, 0418 |
| [0353](0353-calendar-parser.md) | The calendar parser | product | decided | 8/8.1 | 2026-10-09 |  |
| [0354](0354-phase-7-as-built.md) | Phase 7 as-built and the voice's speed | product | decided | 7/7.1–7.4 | 2026-10-09 | 0368, 0372, 0552 |
| [0355](0355-phase-7-as-built-2.md) | Phase 7 as-built, the voice files | product | decided | 7/7.4 | 2026-10-09 |  |
| [0356](0356-phase-8-as-built.md) | Phase 8 as-built | product | decided | 8/8.1–8.5, 8.7 | 2026-10-09 |  |
| [0357](0357-tui-tabs.md) | Thread tabs are the harness TUI | product | decided | 4/4.1, 4.2, 4.8 | 2026-10-09 | 0370 |
| [0358](0358-frontier-fixed-port.md) | Frontier's fixed port | product | decided | 4/4.1 | 2026-10-09 | 0584 |
| [0359](0359-threads-commit-and-report.md) | Workspace threads commit and report | product | decided | 3/3.7 | 2026-10-09 |  |
| [0360](0360-browser-screenshots-in-tabs.md) | Browser screenshots in the thread's TUI | product | decided | 6/6.8 | 2026-10-09 |  |
| [0361](0361-report-blocked-status.md) | `report` can end a thread blocked | product | decided | 3/3.7 | 2026-10-09 |  |
| [0362](0362-v1-beta-choices.md) | Choices from the v1 beta | product | decided | 4/4.4, 4.8, 4.9, 4.12, 7.4 | 2026-10-09 | 0428 |
| [0363](0363-ask-only-beyond-the-plan.md) | Ask only about what goes beyond the plan | process | decided | v1 beta | 2026-10-09 |  |
| [0364](0364-search-and-browser-additions.md) | Search through the headless browser; more browser actions | product | decided | 6/6.8, 6.9 | 2026-10-09 | 0365 |
| [0365](0365-keyless-exa-search.md) | Keyless Exa in `web_search` | product | decided | 6/6.9 | 2026-10-09 |  |
| [0366](0366-frontier-window-chrome-first.md) | Frontier's window opens in Chrome first | product | decided | 4/4.1 | 2026-10-09 | 0367 |
| [0367](0367-frontier-electrobun-app.md) | Frontier becomes an Electrobun desktop app | product | decided | 4/4.1, 4.8, 4.10 | 2026-10-09 | 0368, 0549, 0552, 0557 |
| [0368](0368-electrobun-spike-outcome.md) | Electrobun after the spike, and the global push-to-talk key | product | decided | 4/4.1, 4.8 | 2026-10-09 | 0373, 0493, 0552 |
| [0369](0369-frontier-jarvis-look.md) | Frontier looks like JARVIS, composed | product | decided | 4/4.3, 7.3 | 2026-10-09 | 0401, 0415, 0432, 0548, 0555 |
| [0370](0370-frontier-presence-layout.md) | Frontier's layout: Jarvis presents his crew | product | decided | 4/4.3 | 2026-10-09 | 0408, 0409, 0555 |
| [0371](0371-canvas-draws-by-hand.md) | The canvas draws like a hand | product | decided | 7/7.3 | 2026-10-09 |  |
| [0372](0372-boss-draws.md) | The boss draws | product | decided | 7/7.2, 4/4.4 | 2026-10-10 |  |
| [0373](0373-boss-reads-answers-aloud.md) | The boss reads his answers aloud | product | decided | 7/7.4 | 2026-10-10 | 0405, 0418, 0552 |
| [0374](0374-v1-beta-2-as-built.md) | v1 beta 2: what was fixed as built | product | decided | 2/2.2, 4/4.3–4.4, 6/6.1, 7/7.3 | 2026-10-10 | 0382 |
| [0375](0375-harness-first-measured.md) | Harness first, measured | process | decided | program (all) | 2026-10-10 |  |
| [0376](0376-fake-provider-leaves-product.md) | The fake provider leaves the product | product | decided | 1/1.10 | 2026-10-10 |  |
| [0377](0377-anthropic-through-pi-anthropic-auth.md) | Anthropic through pi-anthropic-auth, Copilot first | product | decided | 5/5.9, 6/6.2 | 2026-10-10 |  |
| [0378](0378-jarvis-everywhere.md) | Jarvis everywhere | product | decided | 0/0.3 | 2026-10-10 |  |
| [0379](0379-canvas-like-whiteboard.md) | The canvas like whiteboard | product | decided | 7/7.2–7.3 | 2026-10-10 |  |
| [0380](0380-review-the-diff.md) | Review the diff after every task | feature | v1 | 4/4.3, 7/7.3 | 2026-10-10 |  |
| [0381](0381-tool-guidance-follows-active-tools.md) | Tool guidance follows the active tools | product | decided | 1/1.11 | 2026-10-10 |  |
| [0382](0382-beta-2-suggestions.md) | Beta-2 suggestions | product | decided | 2/2.2, 4/4.9 | 2026-10-10 |  |
| [0383](0383-fork-only-what-needs-a-patch.md) | Fork only what needs a patch | process | decided | program | 2026-10-13 | 0417, 0564, 0679 |
| [0384](0384-program-on-upstreams.md) | The program on upstreams | process | decided | program | 2026-10-13 |  |
| [0385](0385-installs-for-upstreams.md) | Installs for the upstreams | process | decided | program | 2026-10-13 | 0435 |
| [0386](0386-harness-is-omp-plus-config.md) | The harness is omp plus config | product | decided | program | 2026-10-13 | 0617, 0675 |
| [0387](0387-config-home-daily-omp.md) | The config home is his daily omp | product | decided | program | 2026-10-13 | 0491, 0584, 0650 |
| [0388](0388-threads-are-herdr-worktrees.md) | Threads are herdr worktrees | product | decided | program | 2026-10-13 | 0412 |
| [0389](0389-memory-omp-backend-plus-graphiti.md) | Memory: omp's backend plus Graphiti | product | decided | program | 2026-10-13 | 0400, 0440, 0603, 0604, 0606, 0675 |
| [0390](0390-graphiti-proxy-in-companion.md) | The Graphiti proxy in the companion | product | decided | program | 2026-10-13 | 0463, 0603 |
| [0391](0391-guard-is-omp-approvals.md) | The guard is omp's approvals | product | decided | program | 2026-10-13 | 0450, 0484 |
| [0392](0392-herdr-work-surface-and-companion.md) | herdr is the work surface, with a companion | product | decided | program | 2026-10-13 | 0405, 0409, 0412, 0434, 0495, 0552, 0584, 0649 |
| [0393](0393-whiteboard-canvas-merged.md) | whiteboard's canvas, merged | product | decided | program | 2026-10-13 | 0552, 0607, 0615 |
| [0394](0394-hunk-review-after-every-task.md) | hunk review after every task | feature | v1 | program | 2026-10-13 | 0425, 0679 |
| [0395](0395-ux-pass.md) | The UX pass | process | decided | program | 2026-10-13 |  |
| [0396](0396-omp-settings-walkthrough.md) | The omp settings walkthrough | process | decided | program | 2026-10-13 | 0650 |
| [0397](0397-suggestions-register.md) | The suggestions register | process | decided | program | 2026-10-13 |  |
| [0398](0398-omp-settings-values.md) | omp settings for Jarvis | product | decided | program | 2026-10-13 | 0450, 0451, 0593, 0598 |
| [0399](0399-one-continuous-thread.md) | One continuous thread per worktree | product | decided | program | 2026-10-13 | 0412, 0448, 0592 |
| [0400](0400-project-memory-bank-and-graphiti-feed.md) | One memory bank per project, and what feeds Graphiti | product | decided | program | 2026-10-13 | 0440, 0447, 0449, 0603, 0606 |
| [0401](0401-shared-tokyo-night-theme.md) | One shared theme: Tokyo Night | product | decided | program | 2026-10-13 | 0415, 0432, 0458, 0474 |
| [0402](0402-sidebar-rows.md) | Sidebar rows: state, model, cost | product | decided | program | 2026-10-13 |  |
| [0403](0403-hud-in-tab-bar.md) | The HUD in herdr's tab bar | product | decided | program | 2026-10-13 | 0433, 0437, 0554, 0581 |
| [0404](0404-boss-pane-pinned.md) | The boss pane: a pinned first workspace | product | decided | program | 2026-10-13 |  |
| [0405](0405-events-to-boss-and-voice.md) | Events reach the boss and the voice | product | decided | program | 2026-10-13 | 0413, 0583, 0695 |
| [0406](0406-review-beside-half-width.md) | hunk opens beside the thread | product | decided | program | 2026-10-13 |  |
| [0407](0407-notifications-from-one-place.md) | Notifications from one place | product | decided | program | 2026-10-13 | 0519, 0552, 0554, 0699 |
| [0408](0408-alt-digit-keys.md) | Keys: Alt digits for workspaces | product | decided | program | 2026-10-13 | 0554 |
| [0409](0409-companion-rail-and-canvas.md) | The companion: a rail and the canvas | product | decided | program | 2026-10-13 | 0416, 0419, 0428, 0548 |
| [0410](0410-ui-suggestions-triage.md) | Triage of his UI suggestions | process | decided | program | 2026-10-13 |  |
| [0411](0411-canvas-editors-monaco.md) | Canvas editors: Monaco | product | decided | program | 2026-10-13 |  |
| [0412](0412-thread-is-a-herdr-worktree-workspace.md) | A thread is a herdr worktree workspace | product | decided | program | 2026-10-13 | 0445, 0457 |
| [0413](0413-turn-ends-spoken.md) | Every thread turn end is spoken | product | decided | program | 2026-10-13 | 0583 |
| [0414](0414-thread-branch-prefix.md) | Thread branches are feature/<slug> | product | decided | program | 2026-10-13 | 0455, 0581 |
| [0415](0415-state-colours-follow-herdr.md) | State colours follow herdr | product | decided | program | 2026-10-13 |  |
| [0416](0416-calendar-in-rail.md) | Calendar in the rail | product | decided | program | 2026-10-13 |  |
| [0417](0417-whiteboard-clone-until-submodule.md) | whiteboard as a plain clone until git returns | process | decided | program | 2026-10-13 | 0564 |
| [0418](0418-voice-af-heart-in-service.md) | A more human voice: kokoro af_heart in the service | product | decided | program | 2026-10-14 | 0436, 0552 |
| [0419](0419-canvas-empty-state.md) | The canvas's empty state | product | decided | program | 2026-10-14 | 0432 |
| [0420](0420-omp-config-by-hand.md) | 0398's omp values are set by him | process | decided | program | 2026-10-14 | 0451, 0466, 0593 |
| [0421](0421-ponytail-eval-variant.md) | Ponytail as an eval variant, kept by the rule | process | decided | program | 2026-10-14 |  |
| [0422](0422-no-prompt-diet.md) | No prompt diet: omp's defaults | product | decided | program | 2026-10-14 |  |
| [0423](0423-terminal-bench-after-v1.md) | Terminal-Bench after v1 | process | decided | program | 2026-10-14 |  |
| [0424](0424-boss-pane-config.md) | The boss pane's own omp config | product | decided | program | 2026-10-14 | 0697 |
| [0425](0425-tests-in-review-step.md) | Tests run in the review step | product | decided | program | 2026-10-14 | 0453, 0460, 0484 |
| [0426](0426-thread-finish.md) | thread_finish: merge or PR on his word | product | decided | program | 2026-10-14 | 0453, 0454 |
| [0427](0427-ponytail-kept.md) | Ponytail kept, easy to remove | product | decided | program | 2026-10-14 | 0469, 0577, 0612 |
| [0428](0428-companion-is-the-dashboard.md) | The companion is the dashboard on the second screen | product | decided | program | 2026-10-14 | 0439, 0548, 0557 |
| [0429](0429-dashboard-extras.md) | The dashboard's extra column | product | decided | program | 2026-10-14 | 0439, 0442, 0443, 0444, 0456, 0548, 0555, 0581 |
| [0430](0430-security-hardening.md) | Security hardening beyond the plan | product | decided | program | 2026-10-14 | 0484 |
| [0431](0431-companion-log-and-herdr-restart.md) | Companion log file and herdr restart | product | decided | program | 2026-10-14 | 0472, 0582 |
| [0432](0432-ux-extras.md) | UX extras from the review | product | decided | program | 2026-10-14 | 0440, 0581 |
| [0433](0433-spend-is-all-omp-today.md) | The day's spend is all omp spend | product | decided | program | 2026-10-14 | 0452 |
| [0434](0434-one-life-with-a-guard.md) | Jarvis opens and closes as one, with a guard | product | decided | program | 2026-10-14 | 0438, 0494, 0518, 0519, 0549 |
| [0435](0435-polish-upstream-bumps.md) | Upstream bumps for the polish round | process | decided | program | 2026-10-15 |  |
| [0436](0436-voice-freed-on-mute.md) | The voice leaves memory when muted | product | decided | program | 2026-10-15 | 0552 |
| [0437](0437-hud-every-5s.md) | The HUD every 5 s | product | decided | program | 2026-10-15 | 0456, 0554 |
| [0438](0438-close-question-split-counts.md) | The close question counts working and awaiting threads apart | product | decided | program | 2026-10-15 | 0519, 0582 |
| [0439](0439-dashboard-column-follows-mode.md) | The dashboard's column follows the window's mode | product | decided | program | 2026-10-15 | 0548, 0557 |
| [0440](0440-polish-small-calls.md) | Polish round: small calls | product | decided | program | 2026-10-15 | 0466, 0467 |
| [0441](0441-live-checks-own-herdr-session.md) | Live checks in their own herdr session | process | decided | program | 2026-10-15 |  |
| [0442](0442-news-hacker-news-lwn-gaming.md) | News: Hacker News and LWN in full, gaming 10 | product | decided | program | 2026-10-15 | 0555, 0581, 0668, 0695 |
| [0443](0443-machine-services-every-10s.md) | Machine and services refresh every 10 s | product | decided | program | 2026-10-15 | 0555 |
| [0444](0444-graphiti-on-demand-on-dashboard.md) | Graphiti on the dashboard says it runs on demand | product | decided | program | 2026-10-15 |  |
| [0445](0445-thread-worktrees-beside-project.md) | Thread worktrees beside the project | product | decided | program | 2026-10-15 | 0455, 0457, 0581 |
| [0446](0446-finishing-v1-sandbox-then-clean.md) | Finishing v1: sandbox until agreed, then remove everything installed | process | decided | program | 2026-10-15 | 0468, 0487, 0595 |
| [0447](0447-project-memory-own-file.md) | Each project's memory in its own file, set by the pane settings | product | decided | program | 2026-10-23 | 0478, 0606 |
| [0448](0448-compaction-at-300k-tokens.md) | Compaction in the pane settings, at 300,000 tokens | product | decided | program | 2026-10-23 | 0592 |
| [0449](0449-private-notes-agents-local.md) | Private project notes through AGENTS.local.md | product | decided | program | 2026-10-23 |  |
| [0450](0450-guard-yml-in-every-pane.md) | The guard in every pane: config/guard.yml | product | decided | program | 2026-10-23 | 0476, 0485, 0580, 0598, 0697, 0701 |
| [0451](0451-more-omp-settings.md) | More omp settings for his /settings pass | product | decided | program | 2026-10-23 | 0478, 0479, 0593, 0594 |
| [0452](0452-spend-counts-every-model-call.md) | The day's spend counts every model call | product | decided | program | 2026-10-23 | 0582 |
| [0453](0453-proof-before-done.md) | Proof before done | product | decided | program | 2026-10-23 | 0473, 0513, 0583, 0610, 0611 |
| [0454](0454-merge-after-main-moved.md) | Merging after main moved: bring it in, re-test, re-review | product | decided | program | 2026-10-23 |  |
| [0455](0455-reused-title-new-branch.md) | A reused title gets a new branch | product | decided | program | 2026-10-23 |  |
| [0456](0456-dashboard-hud-threads-quota.md) | What the dashboard and HUD show about threads and quota | product | decided | program | 2026-10-23 | 0554, 0669 |
| [0457](0457-worktree-include-and-thread-env.md) | Worktree setup: copied files and per-thread variables | product | decided | program | 2026-10-23 | 0473, 0477 |
| [0458](0458-herdr-no-manifest-check.md) | herdr fetches no detection files | product | decided | program | 2026-10-23 |  |
| [0459](0459-lessons-at-plan-and-review.md) | Lessons, read at plan and review time, in v1 | product | decided | program | 2026-10-23 | 0471, 0484 |
| [0460](0460-reviewer-on-other-provider.md) | The reviewer runs on the other provider | product | decided | program | 2026-10-23 | 0488, 0583 |
| [0461](0461-tdd-opt-in-lesson.md) | TDD as an opt-in lesson | product | decided | program | 2026-10-23 |  |
| [0462](0462-phone-access-guide-after-v1.md) | Phone access is documented after v1 | product | decided | program | 2026-10-23 |  |
| [0463](0463-graphiti-via-omp-auth-gateway.md) | Graphiti's model calls through omp's auth gateway | product | decided | program | 2026-10-23 | 0582, 0603 |
| [0464](0464-session-files-stay-the-source.md) | Session files stay the source of thread events | product | decided | program | 2026-10-23 |  |
| [0465](0465-boss-procedures-as-skills.md) | Rare boss procedures become skills | product | decided | program | 2026-10-23 | 0583 |
| [0466](0466-setup-updates-as-diff.md) | Setup brings updates as a diff, applied after his y | product | decided | program | 2026-10-23 | 0476, 0479, 0584, 0594, 0694 |
| [0467](0467-setup-refresh-file.md) | `jarvis setup --refresh <file>` | product | decided | program | 2026-10-23 | 0584 |
| [0468](0468-jarvis-uninstall.md) | `jarvis uninstall` | product | decided | program | 2026-10-23 | 0579, 0584, 0595, 0607 |
| [0469](0469-four-test-rules.md) | Four test rules for every thread | product | decided | program | 2026-10-23 | 0577 |
| [0470](0470-weekly-big-picture-review.md) | A weekly big-picture review per active project | product | decided | program | 2026-10-23 | 0577, 0583, 0616, 0619, 0695 |
| [0471](0471-offer-lesson-after-hand-fix.md) | Jarvis offers to save a lesson after a hand-fix or a rejection | product | decided | program | 2026-10-23 | 0583 |
| [0472](0472-jarvis-diagnose.md) | `jarvis diagnose` | product | decided | program | 2026-10-23 | 0583, 0614 |
| [0473](0473-mise-in-project-threads.md) | mise in project threads | product | decided | program | 2026-10-23 | 0476, 0477, 0596 |
| [0474](0474-palette-from-one-file.md) | Tokyo Night colours from one file | product | decided | program | 2026-10-23 |  |
| [0475](0475-agents-md-rules-status-moves-out.md) | AGENTS.md keeps the rules; "Where we are" moves out | process | decided | program | 2026-10-23 |  |
| [0476](0476-mise-single-development-entry.md) | mise as the single entry for development | product | decided | program | 2026-10-23 | 0627, 0630, 0639, 0644 |
| [0477](0477-mise-for-every-project-uncommitted.md) | mise for every project, kept outside the repo | product | decided | program | 2026-10-23 | 0583, 0704 |
| [0478](0478-privacy-settings-in-every-pane.md) | The privacy settings ride in every Jarvis pane | product | decided | program | 2026-10-23 |  |
| [0479](0479-setup-writes-three-omp-files.md) | Setup writes the three omp files after his y | product | decided | program | 2026-10-23 | 0584, 0594 |
| [0480](0480-the-product-is-jocasta.md) | The product is Jocasta | product | decided | program | 2026-10-24 | 0564, 0584 |
| [0481](0481-v1-tested-on-every-surface.md) | v1 is tested on every surface before it is called done | process | decided | program | 2026-10-24 | 0595, 0652 |
| [0482](0482-benchmarks-pre-registered.md) | The benchmarks, pre-registered | process | decided | program | 2026-10-24 | 0488, 0500, 0501, 0508, 0516, 0517, 0520, 0522, 0523, 0528, 0529, 0530, 0533, 0537, 0543, 0574, 0576, 0619, 0720 |
| [0483](0483-thesis-macedonian-outside-repo.md) | The thesis: Macedonian, outside the repo, written by him | process | decided | program | 2026-10-24 | 0507 |
| [0484](0484-security-review-fixes.md) | The security review's fixes | product | decided | program | 2026-10-24 | 0489, 0492, 0519, 0577, 0578, 0579, 0604 |
| [0485](0485-guard-blocks-known-bypasses.md) | The guard blocks the known bypasses; OS isolation after v1 | product | decided | program | 2026-10-24 | 0578, 0580 |
| [0486](0486-closing-a-pane-ends-its-processes.md) | Closing a pane ends everything it started | product | decided | program | 2026-10-24 | 0490 |
| [0487](0487-agent-installs-v1-after-campaign.md) | The cutover: the agent installs v1 after the campaign passes | process | decided | program | 2026-10-24 | 0595 |
| [0488](0488-reviewer-stays-on-anthropic.md) | The reviewer stays on Anthropic | product | decided | program | 2026-10-24 |  |
| [0489](0489-graphiti-network-limit-after-v1.md) | Graphiti's network limit comes after v1 | product | decided | program | 2026-10-24 | 0580 |
| [0490](0490-closing-a-pane-in-herdr-ends-its-leftovers.md) | A thread pane he closes in herdr ends its own leftovers | product | decided | program | 2026-10-24 |  |
| [0491](0491-jocasta-exits-0-after-its-own-stop.md) | `jocasta` exits 0 after Jocasta's own stop | product | decided | program | 2026-10-24 |  |
| [0492](0492-start-box-follows-last-project-pane.md) | The start box follows the last project pane he focused | product | decided | program | 2026-10-24 |  |
| [0493](0493-software-maps-on-page-allows-eval.md) | Software maps on: the page allows eval | product | decided | program | 2026-10-24 |  |
| [0494](0494-plan-review-reopened-after-restart.md) | A plan that awaited him is shown again after a restart | product | decided | program | 2026-10-24 |  |
| [0495](0495-fresh-session-closes-herdrs-default-workspace.md) | A fresh session closes herdr's default workspace | product | decided | program | 2026-10-24 |  |
| [0496](0496-webview2-sign-on-peer-listed.md) | WebView2's account sign-on peer is listed, and looked at again after v1 | product | decided | program | 2026-10-24 |  |
| [0497](0497-installs-inside-the-vm.md) | What the agent installs inside the VM | process | decided | program | 2026-10-27 | 0505, 0652 |
| [0498](0498-sign-ins-copied-into-the-vm.md) | His sign-ins are copied into the VM | process | decided | program | 2026-10-27 | 0504 |
| [0499](0499-the-vm-run-and-the-evidence.md) | Everything runs for real, with an evidence pack per chapter | process | decided | program | 2026-10-27 |  |
| [0500](0500-benchmarks-on-omp-18-13-1.md) | The benchmarks run on omp 18.13.1 | process | decided | program | 2026-10-27 | 0535, 0574, 0585 |
| [0501](0501-h1-corpus-as-built.md) | H1's corpus as built, scored for each way of writing | process | decided | program | 2026-10-27 | 0585 |
| [0502](0502-h4-on-his-pc-in-a-sandbox.md) | H4 runs on his PC, in a sandbox | process | decided | program | 2026-10-27 | 0576 |
| [0503](0503-opencode-in-the-vm.md) | opencode 1.18.22 is installed in the VM | process | decided | program | 2026-10-27 | 0504, 0506 |
| [0504](0504-sign-ins-made-inside-the-vm.md) | His sign-ins are made inside the VM | process | decided | program | 2026-10-28 | 0506 |
| [0505](0505-powershell-7-in-the-vm.md) | PowerShell 7 is installed in the VM | process | decided | program | 2026-10-28 |  |
| [0506](0506-opencode-stays-unsigned.md) | opencode stays unsigned: startup and idle memory only | process | decided | program | 2026-10-28 | 0537 |
| [0507](0507-agent-writes-thesis-and-slides.md) | The agent writes the thesis and the slides at the end | process | decided | program | 2026-11-01 | 0626 |
| [0508](0508-full-run-after-pilot.md) | The full run goes ahead, with three fixes from the pilot | process | decided | program | 2026-11-01 | 0574, 0587 |
| [0509](0509-session-state-follows-active-branch.md) | Session state follows omp's active branch | product | decided | program | 2026-11-01 |  |
| [0510](0510-harness-study-after-full-run.md) | An exploratory harness study after the full run: skill sets and three more arms | process | decided | program | 2026-11-01 |  |
| [0511](0511-analysis-and-thesis-uses.md) | What the analysis adds and what the thesis takes from the research | process | decided | program | 2026-11-01 | 0527 |
| [0512](0512-review-asks-unpiped-test-run.md) | The review asks for the test command unpiped, after the full run | product | decided | program | 2026-11-01 |  |
| [0513](0513-after-v1-product-directions.md) | After v1, product: the review, the brief, tasks and external tools | product | after v1 | after v1 | 2026-11-01 | 0540, 0542, 0583, 0610 |
| [0514](0514-after-v1-platform-directions.md) | After v1, platform: omp², herdr's split, Rust and the companion | product | decided | after v1 | 2026-11-01 |  |
| [0515](0515-playbook-principle-with-omp2.md) | The playbook's five rules become a principle with the omp² move | process | after v1 | after v1 | 2026-11-01 | 0626 |
| [0516](0516-agent-operates-vm-bench-steps.md) | The agent operates the VM benchmarks' manual steps, logged; H8's plans are approved unread | process | decided | program | 2026-11-01 | 0525, 0576 |
| [0517](0517-benchmarks-on-the-fixed-build.md) | Jocasta's benchmarks run on a build with herdr's dropped reads retried, and the done ones run again | process | decided | program | 2026-11-01 | 0520, 0576 |
| [0518](0518-dead-herdr-server-heals.md) | A herdr server that dies under `jocasta`'s terminal is started again and its client attached again | product | decided | program | 2026-11-02 | 0565, 0566, 0649 |
| [0519](0519-jocasta-own-windows-for-questions-and-toasts.md) | The close question, the Allow box and the toasts become Jocasta's own windows | product | decided | program | 2026-11-02 | 0524, 0552 |
| [0520](0520-all-jocasta-rows-on-the-0518-0519-build.md) | Every Jocasta row runs again on the build with 0518 and 0519 | process | decided | program | 2026-11-02 | 0576 |
| [0521](0521-omp-ended-in-open-pane-starts-again.md) | An omp that ends in an open Jocasta pane is started again | product | decided | program | 2026-11-02 |  |
| [0522](0522-h3-recovered-when-omp-is-back.md) | H3 counts a herdr or pane fault recovered when omp is back in every workspace | process | decided | program | 2026-11-02 |  |
| [0523](0523-h3-session-check-fixed.md) | H3's session check accepts a session entry anywhere in the file | process | decided | program | 2026-11-02 |  |
| [0524](0524-question-windows-leave-the-keyboard.md) | The Allow window and the close question leave the keyboard with his app | product | decided | program | 2026-11-02 | 0552 |
| [0525](0525-vm-firewall-prompt-cancelled.md) | The agent cancels the VM's Firewall prompt for Node.js | process | decided | program | 2026-11-02 | 0526 |
| [0526](0526-vm-firewall-prompts-cancelled.md) | Every Firewall prompt in the VM is cancelled and noted | process | decided | program | 2026-11-02 |  |
| [0527](0527-playbook-and-omp2-thesis-sections.md) | The thesis gives the playbook and omp² their own sections and cites three voices of the new wave | process | decided | program | 2026-11-02 |  |
| [0528](0528-h3-faults-spaced-and-companion-rule.md) | H3 kills each folder's omp at most once in 10 minutes, the companion fault counts omp back everywhere, and each restart's client ends | process | decided | program | 2026-11-02 | 0530 |
| [0529](0529-h8-finished-with-a-commit.md) | H8 counts a thread finished when it is idle or done with a commit on its branch | process | decided | program | 2026-11-02 | 0531, 0576 |
| [0530](0530-bugs-after-benchmarks-h3-restarts.md) | The reruns' three product bugs are fixed after the benchmarks, H3 starts a Jocasta that closed itself again, and H3 and H8 run attended | process | decided | program | 2026-11-02 | 0531, 0534 |
| [0531](0531-h8-eight-threads-rerun-once-clean.md) | H8's 8-thread run runs once more on a clean sandbox, and running out of memory again is its result | process | decided | program | 2026-11-02 |  |
| [0532](0532-harness-os-in-v1-before-benchmarks.md) | The harness-OS layer is v1, built before the benchmarks | product | v1 | program | 2026-11-08 |  |
| [0533](0533-all-benchmarks-rerun-on-new-build.md) | Every benchmark runs again on the harness-OS build | process | decided | program | 2026-11-08 | 0625, 0652, 0657 |
| [0534](0534-0530-bugs-fixed-in-new-build.md) | The reruns' three product bugs are fixed in the harness-OS build | process | decided | program | 2026-11-08 |  |
| [0535](0535-omp-latest-at-build-time.md) | The harness-OS build pins the omp that is latest when its build starts | process | decided | program | 2026-11-08 | 0589 |
| [0536](0536-omp-threads-driven-through-local-collab-relay.md) | Jocasta runs a local collab relay for thread state; it joins as a full guest only if a spike shows it's worth keeping | product | v1 | program | 2026-11-08 | 0675 |
| [0537](0537-omp-only-competitors-bare.md) | Jocasta runs omp threads only; opencode and Claude Code are benchmarked bare | product | decided | program | 2026-11-08 | 0574, 0657 |
| [0538](0538-k1-cost-ledger.md) | K1: the cost ledger is a projection of the session files, on omp's cost, shown through thread_status | product | v1 | program | 2026-11-08 | 0546, 0576, 0582 |
| [0539](0539-k4-report-result-and-k3-decisions.md) | K4 and K3: threads hand in a structured report through report_result, and its decisions go to Graphiti and lesson offers | product | v1 | program | 2026-11-08 | 0577, 0603 |
| [0540](0540-k2-task-file-done-when-and-brief-review.md) | K2: a task file per thread, its brief and done-when given at launch, and done_when checked in the review | product | v1 | program | 2026-11-08 | 0577, 0580, 0621 |
| [0541](0541-k7-k10-omp-covers-tools-models-and-budgets.md) | K7–K10: omp already covers tool lists and models per phase; prewalk and keep-alive pings are measured; each thread warns at $50 | product | v1 | program | 2026-11-08 | 0575, 0583, 0586 |
| [0542](0542-k5-index-and-k6-checks-and-dev-servers.md) | K5 and K6: a shared code index behind graph_search, cached service-side checks, and dev servers, each tried and kept if it measures better | product | v1 | program | 2026-11-08 | 0546, 0571, 0577, 0582, 0604, 0605, 0610 |
| [0543](0543-benchmark-design-for-harness-os-rerun.md) | The rerun's design: seven arms, a follow-up study of the layer's parts, H8 extended with four threads, a keep rule, and a pacer that waits for every account | process | decided | program | 2026-11-08 | 0545, 0567, 0571, 0574, 0575, 0576, 0577, 0587, 0605, 0620 |
| [0544](0544-deterministic-thread-commands-and-buttons.md) | Thread work without a model turn: `jocasta thread` and `worktree` commands, and Finish and Stop buttons | product | v1 | program | 2026-11-08 | 0552, 0568, 0580, 0581, 0649 |
| [0545](0545-fresh-context-and-ponytail-if-measured.md) | Fresh context after the plan and Ponytail's conditional test block are measured in the follow-up study | process | decided | program | 2026-11-08 | 0575 |
| [0546](0546-peak-context-and-gate-replies.md) | The ledger shows peak context, and checks answer as gates with a rerun command | product | v1 | program | 2026-11-08 | 0577, 0582, 0583 |
| [0547](0547-thread-state-from-omp-first.md) | A thread's state comes from omp first, and the unused thread_runtime table is dropped | product | v1 | program | 2026-11-08 |  |
| [0548](0548-herdr-is-the-whole-surface-companion-is-the-canvas.md) | herdr becomes the whole work surface: the rail moves into a herdr pane, the companion window becomes the canvas, one screen each | product | v1 | program | 2026-11-08 | 0552, 0555, 0581 |
| [0549](0549-nothing-runs-without-jocasta.md) | Nothing runs without Jocasta: no Keep in the background, autostart off by default, paused work resumes on the next start | product | v1 | program | 2026-11-08 | 0552, 0557, 0563, 0702 |
| [0550](0550-omp-updates-off-and-setup-files-now.md) | omp stops updating itself until the benchmarks end, and setup's omp files are written on his PC now | process | decided | program | 2026-11-08 |  |
| [0551](0551-jocasta-pane-drawn-with-opentui.md) | The `jocasta` pane is drawn with OpenTUI and holds today, the thread keys and his answers | product | v1 | program | 2026-11-09 | 0581, 0625, 0634 |
| [0552](0552-no-companion-app.md) | No companion app: the service is a plain Bun process, diagrams open in the browser, dictation stays without spoken replies, and questions are answered in herdr | product | v1 | program | 2026-11-09 | 0553, 0556, 0563, 0578, 0583, 0584, 0649, 0672 |
| [0553](0553-jocasta-is-a-herdr-plugin.md) | Jocasta is a herdr plugin: `today` beside the boss, questions in a popup, after a spike | product | v1 | program | 2026-11-09 | 0558, 0578, 0649, 0667, 0717 |
| [0554](0554-herdr-keys-pings-and-settings.md) | herdr's keys, pings and settings for Jocasta | product | v1 | program | 2026-11-09 | 0569, 0578, 0581, 0667 |
| [0555](0555-today-pane-contents.md) | The `today` pane's contents: what needs him first, no greeting or Presence, the extras where they fit, no screen hotkeys | product | v1 | program | 2026-11-09 | 0581, 0668, 0670 |
| [0556](0556-dictation-through-omp-stt.md) | Dictation goes through omp's own speech-to-text in the boss pane, started from anywhere with Right Ctrl | product | v1 | program | 2026-11-09 |  |
| [0557](0557-settings-review.md) | The settings review: the app-only settings go, everything else stays | product | v1 | program | 2026-11-09 | 0584 |
| [0558](0558-herdr-plugin-spike-and-answer-code.md) | The herdr plugin works on Windows; a popup doesn't stop socket keys, so Allow takes a typed code | product | v1 | program | 2026-11-09 | 0568, 0578, 0672 |
| [0559](0559-mise-cache-spike.md) | mise's task cache keeps passing checks; Jocasta keys results by git tree, keeps failures and runs one check per tree | product | v1 | program | 2026-11-09 | 0582 |
| [0560](0560-pitchfork-spike.md) | pitchfork isn't kept: dev servers stay plain background processes | product | dropped | program | 2026-11-09 |  |
| [0561](0561-relay-spike.md) | The local relay works: thread state comes from omp's host pipes, panes refuse to start unless the relay is loopback, and no full guest | product | v1 | program | 2026-11-09 | 0675 |
| [0562](0562-code-index-spike.md) | The code index is measured on each project's main checkout, for relationship questions only | product | v1 | program | 2026-11-09 | 0604, 0605 |
| [0563](0563-closing-the-terminal-reopens-and-asks.md) | Closing the terminal while threads work reopens it and asks before stopping | product | v1 | program | 2026-11-09 | 0568 |
| [0564](0564-version-control-returns.md) | Version control returns: git in the repo, the agent commits finished phases under a repo-only identity, a public `jocasta` repository, whiteboard as a submodule, and the folder renamed after the build | process | decided | program | 2026-11-09 | 0615 |
| [0565](0565-hung-service-no-longer-blocks-starts.md) | A hung service no longer blocks every start | product | v1 | program | 2026-11-09 | 0582 |
| [0566](0566-herdr-crash-is-not-quitting.md) | A herdr crash the service healed is no longer read as him quitting | product | v1 | program | 2026-11-09 |  |
| [0567](0567-a6-approves-like-the-product-and-pings-only-in-their-test.md) | A6 approves its plan as the product does, and only the keep-alive test pings | process | v1 | program | 2026-11-09 | 0575, 0577, 0619 |
| [0568](0568-finish-and-quit-take-the-code.md) | Merge, PR and Stop everything take the typed code too | product | v1 | program | 2026-11-09 | 0578 |
| [0569](0569-finished-threads-chime.md) | Finished threads chime with herdr's done sound | product | v1 | program | 2026-11-09 |  |
| [0570](0570-parallel-starts-share-the-overlay.md) | Parallel thread starts in one project no longer race on its omp.yml | product | v1 | program | 2026-11-09 |  |
| [0571](0571-no-check-arm-in-the-follow-up.md) | The follow-up study drops f-check-off; A6's check stays as the product behaves | process | v1 | program | 2026-11-09 | 0575, 0576 |
| [0572](0572-linux-and-macos-pass-ci-now.md) | CI's Linux and macOS jobs pass before the benchmarks | process | v1 | program | 2026-11-09 | 0584 |
| [0573](0573-full-review-before-the-benchmarks.md) | A full review and its fixes before the benchmarks; a week of use, then the benchmarks | process | v1 | program | 2026-11-09 |  |
| [0574](0574-task-benchmark-grades-what-the-agent-broke.md) | The task benchmark grades what the agent broke, has limits, and holds its conditions | benchmark | v1 | program | 2026-11-09 |  |
| [0575](0575-follow-up-drops-f-keepalive.md) | The follow-up drops f-keepalive; A6's layer builds its code index before the trial's clock | benchmark | v1 | program | 2026-11-09 | 0586 |
| [0576](0576-live-benchmarks-on-the-harness-os-build.md) | The live benchmarks on the harness-OS build | benchmark | v1 | program | 2026-11-09 | 0585 |
| [0577](0577-thread-prompts-and-tool-time-limits.md) | Thread prompts and the tools' time limits | product | v1 | program | 2026-11-09 | 0662 |
| [0578](0578-allow-popup-and-local-api.md) | The Allow popup and the service's local API | product, security | v1 | program | 2026-11-09 | 0699 |
| [0579](0579-install-folder-is-jocastas-alone.md) | The install folder is Jocasta's alone | product | v1 | program | 2026-11-09 |  |
| [0580](0580-guard-covers-more-shapes.md) | The guard covers more shapes, and done_when goes through it | product, security | v1 | program | 2026-11-09 | 0613, 0701 |
| [0581](0581-today-keys-wording-and-canvas.md) | today, the keys, the wording and the canvas | product, ux | v1 | program | 2026-11-09 | 0660, 0662, 0670, 0717 |
| [0582](0582-starting-recovering-and-counting-cost.md) | Starting, recovering and counting cost | product | v1 | program | 2026-11-09 | 0649 |
| [0583](0583-boss-manual-and-shipped-prompt-texts.md) | The boss manual and shipped prompt texts | product, prompts | v1 | program | 2026-11-09 | 0608, 0611, 0616, 0619, 0621 |
| [0584](0584-setup-settings-tests-names-and-stale-files.md) | Setup, settings, tests, names and stale files | product, process | v1 | program | 2026-11-09 |  |
| [0585](0585-h1-live-check-gets-a-new-recorder.md) | H1's live check gets a new recorder on omp 18.18.2 | benchmark | v1 | program | 2026-11-09 |  |
| [0586](0586-keep-alive-pings-are-not-measured.md) | Keep-alive pings are not measured | benchmark | v1 | program | 2026-11-09 |  |
| [0587](0587-bench-refuses-omp-folder-mcp-servers.md) | The task benchmark refuses an omp folder that gives its omp arms MCP servers | benchmark | v1 | program | 2026-11-09 | 0652 |
| [0588](0588-benchmark-scripts-run-once-first.md) | Each benchmark script runs once before the full runs | benchmark | v1 | program | 2026-11-09 |  |
| [0589](0589-omp-18-18-6.md) | Jocasta moves to omp 18.18.6 | product, benchmark | v1 | program | 2026-11-10 | 0600 |
| [0590](0590-models-per-role-after-fable-5-2.md) | Models per role after Fable 5.2: the boss on Fable, heavy threads on his slow role | product, benchmark | v1 | program | 2026-11-10 | 0655, 0696 |
| [0591](0591-subagents-on-sonnet-memory-and-graph-unchanged.md) | Subagents on Sonnet 5.5; memory notes and the graph unchanged | product | v1 | program | 2026-11-10 | 0603 |
| [0592](0592-compaction-handoff-first.md) | Compaction: handoff first, shake and the image method last | product | v1 | program | 2026-11-10 | 0677 |
| [0593](0593-his-settings-pass.md) | His /settings pass: the values in his config and in Jocasta's | product | v1 | program | 2026-11-10 | 0616, 0619 |
| [0594](0594-setup-fences-folders-and-writes-secrets.md) | Setup fences the key folders and Windows' own, and writes an empty secrets.yml | product | v1 | program | 2026-11-10 | 0701 |
| [0595](0595-cutover-now.md) | The cutover now: Jarvis is Jocasta, everything from Jarvis is deleted | process | v1 | program | 2026-11-10 |  |
| [0596](0596-worktrees-trust-unchanged-mise-copy.md) | A thread's worktree trusts an unchanged copy of a trusted mise config | product | v1 | program | 2026-11-10 |  |
| [0598](0598-git-guard-off-until-omp-fixes-windows-git.md) | The git guard is off until omp's git builtin runs git on Windows | product | v1 | program | 2026-11-10 | 0600 |
| [0599](0599-tune-before-the-vm-weekend.md) | Tune first, then the VM weekend | process | v1 | program | 2026-11-14 | 0625 |
| [0600](0600-omp-pin-18-20-0.md) | Jocasta moves to omp 18.20.0 | product, benchmark | v1 | program | 2026-11-14 | 0623 |
| [0601](0601-completion-probe-off-in-panes.md) | The completion probe is off in Jocasta's panes | product | v1 | program | 2026-11-14 |  |
| [0602](0602-job-widgets-in-the-boss-pane-only.md) | Background-job widgets stay in the boss pane only | product | v1 | program | 2026-11-14 |  |
| [0603](0603-graphiti-out-past-sessions-searched-in-sqlite.md) | Graphiti and FalkorDB go; past sessions are searched in SQLite | product, benchmark | v1 | program | 2026-11-14 |  |
| [0604](0604-the-search-tool-is-history-search.md) | The search tool is named `history_search` | product | v1 | program | 2026-11-14 |  |
| [0605](0605-the-code-index-is-removed.md) | The code index is removed | product, benchmark | v1 | program | 2026-11-14 |  |
| [0606](0606-mnemopi-off-in-jocastas-panes.md) | Mnemopi is off in Jocasta's panes | product | v1 | program | 2026-11-14 |  |
| [0607](0607-whiteboard-tools-for-the-boss-only.md) | Whiteboard's tools are mounted for the boss only | product | v1 | program | 2026-11-14 | 0679 |
| [0608](0608-tuning-program-small-calls.md) | The tuning program's small calls | product, benchmark | v1 | program | 2026-11-14 | 0620, 0622 |
| [0609](0609-the-code-quality-standard.md) | The code-quality standard: the five sources, Ousterhout where they disagree | process | v1 | program | 2026-11-14 |  |
| [0610](0610-quality-held-by-machine-checks.md) | Quality is held by machine checks first | product, process | v1 | program | 2026-11-14 | 0617, 0645 |
| [0611](0611-a-design-section-in-the-review.md) | The review gets a design section; the weekly review gets the same rubric | product | v1 | program | 2026-11-14 | 0707 |
| [0612](0612-ponytail-two-lines-rewritten.md) | Ponytail: two lines rewritten, one removed | product, prompts | v1 | program | 2026-11-14 |  |
| [0613](0613-pushes-to-trunk-ask-once-per-repo.md) | A push to main, master or develop asks him once per repo | product, security | v1 | program | 2026-11-14 |  |
| [0614](0614-quality-and-security-tools.md) | Quality and security tools: what enters the flow, what stays on demand | product, process | v1 | program | 2026-11-14 | 0624, 0627, 0639, 0645, 0648 |
| [0615](0615-whiteboard-0-5-2-with-animations.md) | Whiteboard moves to v0.5.2 before the benchmarks, with animations on | product | v1 | program | 2026-11-14 | 0679 |
| [0616](0616-tuning-program-small-calls-2.md) | Tuning program: four more small calls | product, process | v1 | program | 2026-11-14 | 0695 |
| [0617](0617-project-pane-check-through-omps-turn-end-event.md) | A failed check reaches a project pane's agent through omp's turn-end event | product | v1 | program | 2026-11-15 | 0675 |
| [0618](0618-settings-decided-by-trials-builds-wait.md) | Settings are decided by trials; the builds wait until everything is settled | process | v1 | program | 2026-11-15 |  |
| [0619](0619-omp-settings-decided-by-the-trials.md) | The omp settings the trials decided | product, benchmark | v1 | program | 2026-11-15 | 0651, 0695 |
| [0620](0620-advisor-stays-follow-up-study-gains-two-arms.md) | The advisor stays; the follow-up study gains an advisor arm and a thinking arm | product, benchmark | v1 | program | 2026-11-15 |  |
| [0621](0621-finished-threads-record-their-close-and-commit.md) | A finished thread records when it closed and its commit; the weekly review gets production numbers | product | v1 | program | 2026-11-15 |  |
| [0622](0622-tuning-program-small-calls-3.md) | Tuning program: two small calls from the factory view | process | v1 | program | 2026-11-15 |  |
| [0623](0623-omp-18-20-2-everywhere.md) | omp 18.20.2 everywhere; its selective-commit text stays on | product, benchmark | v1 | program | 2026-11-15 | 0624, 0651 |
| [0624](0624-omp-installed-at-any-age-0617-reviewed-again.md) | omp's install is outside the package-age rule; 0617 is reviewed again | process | v1 | program | 2026-11-15 |  |
| [0625](0625-rust-rewrite-before-the-benchmarks.md) | Everything Jocasta owns is rewritten in Rust, before the benchmarks | product | v1 | program | 2026-11-15 | 0636 |
| [0626](0626-thesis-writing-starts-now.md) | The thesis is written now, in FINKI's shape with two more chapters; the craft becomes a principle | process, product | v1 | program | 2026-11-15 |  |
| [0627](0627-rust-toolchain-and-crate-age.md) | Rust 1.100.0 through mise; crates follow the 7-day rule | process, product | v1 | program | 2026-11-15 | 0640 |
| [0628](0628-rust-port-beside-then-one-cutover.md) | The Rust build grows beside the TypeScript one; one cutover at the end | process | v1 | program | 2026-11-15 |  |
| [0629](0629-one-cargo-package-beside-the-typescript.md) | One Cargo package, with each Rust module beside the TypeScript file it replaces | product | v1 | program | 2026-11-15 |  |
| [0630](0630-rust-tests-and-fakes.md) | The tests are ported with their modules, and the fakes are rewritten in Rust | process | v1 | program | 2026-11-15 |  |
| [0631](0631-tokio-and-axum-for-the-service.md) | tokio and axum for the service's HTTP and websockets | product | v1 | program | 2026-11-15 |  |
| [0632](0632-rmcp-for-jocasta-mcp.md) | rmcp serves `jocasta mcp` | product | v1 | program | 2026-11-15 |  |
| [0633](0633-reqwest-for-http-calls.md) | reqwest for HTTP calls | product | v1 | program | 2026-11-15 |  |
| [0634](0634-ratatui-for-today-and-ask.md) | ratatui draws `today` and the `ask` popup | product | v1 | program | 2026-11-15 |  |
| [0635](0635-saphyr-for-yaml.md) | saphyr reads and writes YAML | product | v1 | program | 2026-11-15 |  |
| [0636](0636-bench-and-evals-ported-as-a-second-binary.md) | bench/ and evals/ are ported to Rust as a second binary | product, benchmark | v1 | program | 2026-11-15 |  |
| [0637](0637-clap-for-the-command-words.md) | clap parses the `jocasta` command's words | product | v1 | program | 2026-11-15 |  |
| [0638](0638-icalendar-for-calendars.md) | icalendar reads the calendar feeds | product | v1 | program | 2026-11-15 |  |
| [0639](0639-the-ports-remaining-crates.md) | The port's remaining crates and tools | product, process | v1 | program | 2026-11-15 | 0640, 0641, 0642, 0647 |
| [0640](0640-port-helper-crates-and-the-first-lock.md) | The port's helper crates; the first lock takes the newest versions | product, process | v1 | program | 2026-11-15 |  |
| [0641](0641-process-trees-end-two-ways.md) | Process trees end two ways: process-wrap for Jocasta's own programs, the snapshot walk for the rest | product | v1 | program | 2026-11-15 |  |
| [0642](0642-notify-leaves-the-port.md) | notify leaves the port; the session watcher keeps its timer | product | v1 | program | 2026-11-15 |  |
| [0643](0643-rust-in-wsl-for-linux-tests.md) | Rust 1.100.0 in his WSL Ubuntu for the port's Linux test runs | process | v1 | program | 2026-11-15 |  |
| [0644](0644-cargo-nextest-runs-the-rust-tests.md) | cargo-nextest runs the Rust tests | process | v1 | program | 2026-11-15 |  |
| [0645](0645-quality-checks-after-the-cutover.md) | The quality checks after the cutover | process | v1 | program | 2026-11-16 |  |
| [0646](0646-readme-for-newcomers.md) | A README for newcomers, the reference in the manual | product | v1 | program | 2026-11-17 |  |
| [0647](0647-deny-ignores-chrono-unmaintained.md) | cargo-deny ignores chrono's two "unmaintained" notices by ID | process | v1 | program | 2026-11-18 |  |
| [0648](0648-audit-ignores-picomatch-redos.md) | CI's `bun audit` ignores picomatch's ReDoS advisory by ID | process | v1 | program | 2026-11-18 |  |
| [0649](0649-herdr-fork-one-binary-one-server.md) | The herdr fork: one binary, one server | product | v1 | fork build | 2026-11-18 | 0659, 0661, 0665, 0666, 0683 |
| [0650](0650-omp-snapshot-replaces-his-omp-files.md) | The omp snapshot: setup writes Jocasta's omp files over his | product | v1 | fork build | 2026-11-18 | 0696, 0697 |
| [0651](0651-snapshot-settings-from-trials-on-omp-18-22-2.md) | The snapshot's settings come from trials on omp 18.22.2 | product, benchmark | v1 | fork build | 2026-11-19 | 0653, 0696 |
| [0652](0652-benchmarks-on-his-pc-vm-removed.md) | The benchmarks run on his PC on his own omp; the VM is removed | process | v1 | fork build | 2026-11-18 | 0657 |
| [0653](0653-omp-19-1-0-pinned.md) | omp 19.1.0 is the pinned omp | product, benchmark | v1 | fork build | 2026-11-21 | 0663 |
| [0654](0654-fork-build-review.md) | His review of the fork build: what stays as built | process, product | v1 | fork build | 2026-11-23 |  |
| [0655](0655-normal-threads-on-opus-5-5-medium.md) | Normal threads on Opus 5.5 at medium | product | v1 | fork build | 2026-11-23 |  |
| [0656](0656-url-2-5-9-before-its-seventh-day.md) | url 2.5.9 enters the lock before its seventh day | process | v1 | fork build | 2026-11-23 |  |
| [0657](0657-vm-benchmarks-after-the-vm.md) | The VM's benchmarks after the VM | benchmark, process | v1 | fork build | 2026-11-23 | 0720 |
| [0658](0658-session-report-from-his-omp-history.md) | A report on his omp sessions: where the tokens went, what could be deterministic | process | v1 | fork build | 2026-11-23 |  |
| [0659](0659-sidebar-without-spaces.md) | The sidebar without herdr's spaces section | product | v1 | fork build | 2026-11-23 | 0664, 0683 |
| [0660](0660-a-click-on-a-today-row-acts-like-enter.md) | A click on a `today` row acts like Enter | product, ux | v1 | fork build | 2026-11-23 | 0683 |
| [0661](0661-bundled-conpty-runtime.md) | The Windows build ships Microsoft's ConPTY runtime, as herdr's does | product | v1 | fork build | 2026-11-23 | 0683 |
| [0662](0662-a-silent-service-is-shown-and-cut.md) | A service that stops answering is shown in the dock, and the boss's calls to it end | product | v1 | fork build | 2026-11-23 | 0683 |
| [0663](0663-omp-19-1-1-pinned.md) | omp 19.1.1 is the pinned omp | product, benchmark | v1 | fork build | 2026-11-23 | 0683, 0690 |
| [0664](0664-the-sidebar-strip-keeps-only-menu.md) | The sidebar's top strip keeps only `menu` | product, ux | v1 | fork build | 2026-11-23 | 0667, 0683 |
| [0665](0665-jocasta-owns-the-workspaces.md) | Jocasta owns the workspaces | product, ux | v1 | fork build | 2026-11-23 | 0683 |
| [0666](0666-herdrs-leftovers-pruned.md) | herdr's leftovers pruned, the menu and help in Jocasta's words | product, ux | v1 | fork build | 2026-11-23 | 0683 |
| [0667](0667-the-boss-row-pinned-in-the-sidebar.md) | The boss's row pinned first in the sidebar | product, ux | v1 | HUD round | 2026-11-23 |  |
| [0668](0668-the-dock-news-box-and-thread-facts.md) | The dock: a five-line news box, and each thread's diff, check, PR and stuck state | product, ux | v1 | HUD round | 2026-11-23 |  |
| [0669](0669-quota-for-every-account.md) | Quota for every logged-in account | product, ux | v1 | HUD round | 2026-11-23 |  |
| [0670](0670-leftover-worktrees-in-the-dock.md) | Leftover worktrees in the dock, removed behind his Allow | product, ux | v1 | HUD round | 2026-11-23 |  |
| [0671](0671-the-check-queue-learns-what-checks-cost.md) | The check queue learns what each check costs | product | v1 | HUD round | 2026-11-23 |  |
| [0672](0672-phone-check-in-through-ntfy.md) | Phone check-in through ntfy, with Allow from the phone | product | v1 | HUD round | 2026-11-23 |  |
| [0673](0673-scheduled-threads.md) | Scheduled threads | product | v1 | HUD round | 2026-11-23 |  |
| [0674](0674-the-lightweight-program-and-its-order.md) | The lightweight program, its order and its footprint budget | product, process | v1 | lightweight program | 2026-11-24 | 0689, 0720 |
| [0675](0675-the-bridge-one-extension-one-pipe.md) | The bridge: one omp extension and one local pipe | product | v1 | lightweight program | 2026-11-24 | 0683, 0702 |
| [0676](0676-process-diet.md) | Process diet: panes start on demand and sleep; a footprint that only goes down | product | v1 | lightweight program | 2026-11-24 | 0689 |
| [0677](0677-deterministic-loops.md) | Deterministic validation loops | product | v1 | lightweight program | 2026-11-24 |  |
| [0678](0678-briefs-as-html-pages.md) | Briefs: animated HTML pages served on loopback | product | v1 | lightweight program | 2026-11-24 | 0698, 0699, 0706, 0707, 0710, 0712, 0713, 0721 |
| [0679](0679-whiteboard-and-hunk-removed.md) | Whiteboard and hunk removed; `show` draws a brief | product | v1 | lightweight program | 2026-11-24 | 0714 |
| [0680](0680-day-flows-and-new-sources.md) | Day flows and three new read-only sources | product | v1 | lightweight program | 2026-11-24 | 0706, 0707, 0709, 0711, 0712 |
| [0681](0681-privacy-boundary.md) | Privacy boundary: one door to the model | product | v1 | lightweight program | 2026-11-24 |  |
| [0682](0682-validation-environment-mise-tasks.md) | Validation environment: the project's own mise tasks | process | v1 | lightweight program | 2026-11-24 |  |
| [0683](0683-own-shell-on-alacritty-terminal.md) | Own shell on `alacritty_terminal`, gated by a spike | product | v1 | lightweight program | 2026-11-24 | 0702, 0717 |
| [0684](0684-harness-contract-and-omp2-preparation.md) | Harness contract: omp's own screen, capabilities, no adapter layer | product | v1 | lightweight program | 2026-11-24 |  |
| [0685](0685-stage-setting-facts-and-precheck.md) | Stage setting: a facts block by code and the full check ahead of time | product | v1 | lightweight program | 2026-11-24 |  |
| [0686](0686-circuit-breakers.md) | Circuit breakers: loop cap, cost cap, pressure sleep, stop-all | product | v1 | lightweight program | 2026-11-24 |  |
| [0687](0687-thread-dependencies-and-running-limit.md) | Thread dependencies (`after`) and a running limit of 4 | product | v1 | lightweight program | 2026-11-24 |  |
| [0688](0688-the-journal.md) | The journal: one append-only file of events | product | v1 | lightweight program | 2026-11-24 |  |
| [0689](0689-sleeping-panes-in-process-and-footprint-count.md) | Sleeping panes drawn by the shell; the footprint counts Jocasta's own processes | product | v1 | lightweight program | 2026-11-25 |  |
| [0690](0690-omp-19-2-0-pinned.md) | omp 19.2.0 is the pinned omp | product, benchmark | v1 | lightweight program | 2026-11-26 | 0693 |
| [0691](0691-releases-start-at-v0-1-0-mise-installs-them.md) | Releases start at v0.1.0; mise installs them | product | v1 | ultron 0.3 iteration, J10 | 2026-11-26 |  |
| [0692](0692-unix-install-folder.md) | Unix install folder | product | v1 | ultron 0.3 iteration, J1 | 2026-11-26 |  |
| [0693](0693-omp-is-not-pinned.md) | omp is not pinned | product, process | v1 | ultron 0.3 iteration, J10 | 2026-11-26 |  |
| [0694](0694-jocasta-setup-yes.md) | `jocasta setup --yes` | product | v1 | ultron 0.3 iteration, J6 | 2026-11-26 |  |
| [0695](0695-light-defaults.md) | Light defaults | product | v1 | ultron 0.3 iteration, J2 | 2026-11-26 |  |
| [0696](0696-snapshot-local-commit-model-no-sleep-inhibitor-on-linux.md) | Snapshot: local commit model, no sleep inhibitor on Linux | product | v1 | ultron 0.3 iteration, J3 | 2026-11-26 | 0711 |
| [0697](0697-personal-overlays.md) | Personal overlays | product | v1 | ultron 0.3 iteration, J4 | 2026-11-26 |  |
| [0698](0698-palette-file.md) | Palette file | product | v1 | ultron 0.3 iteration, J7 | 2026-11-26 | 0721 |
| [0699](0699-linux-notifications.md) | Linux notifications | product | v1 | ultron 0.3 iteration, J1 | 2026-11-26 |  |
| [0700](0700-jocasta-status-json.md) | `jocasta status --json` | product | v1 | ultron 0.3 iteration, J5 | 2026-11-26 |  |
| [0701](0701-linux-guard-shapes-and-fenced-folders.md) | Linux guard shapes and fenced folders | product | v1 | ultron 0.3 iteration, J1 | 2026-11-26 |  |
| [0702](0702-unix-runtime.md) | Unix runtime | product | v1 | ultron 0.3 iteration, J1 | 2026-11-26 |  |
| [0703](0703-first-run-checks.md) | First-run checks | product | v1 | ultron 0.3 iteration, J8 | 2026-11-26 |  |
| [0704](0704-project-named-mise-file.md) | Project-named mise file | product | v1 | ultron 0.3 iteration, J9 | 2026-11-26 |  |
| [0705](0705-ponytail-text-refreshed.md) | Ponytail text refreshed | product, prompts | v1 | ultron 0.3 iteration, J10 | 2026-11-26 |  |
| [0706](0706-review-works-first.md) | Review: works first | product, prompts | v1 | ultron 0.3 iteration, M2 R2 | 2026-11-26 |  |
| [0707](0707-review-proof-on-every-finding.md) | Review: proof on every finding | product, prompts | v1 | ultron 0.3 iteration, M2 R3 | 2026-11-26 |  |
| [0709](0709-review-panes-ask-before-running.md) | Review panes ask before running | product, security | v1 | ultron 0.3 iteration, M2 R4 | 2026-11-26 |  |
| [0710](0710-review-system-view.md) | Review: system view | product, prompts | v1 | ultron 0.3 iteration, M2 R5 | 2026-11-26 |  |
| [0711](0711-polish-to-english-own-comments.md) | Polish, To English, his own comments | product, prompts | v1 | ultron 0.3 iteration, M2 R6 | 2026-11-26 |  |
| [0712](0712-submit-with-a-verdict-one-page.md) | Submit with a verdict; one page for threads and pull requests | product | v1 | ultron 0.3 iteration, M2 R6 | 2026-11-26 |  |
| [0713](0713-page-motion-and-design.md) | Page motion and design | product | v1 | ultron 0.3 iteration, M2 R7 | 2026-11-26 |  |
| [0708](0708-clean-room-runs.md) | Clean-room runs | product, security | v1 | ultron 0.3 iteration, M2 R1 | 2026-11-26 |  |
| [0721](0721-native-views-with-iced.md) | Native views with iced | product, security | v1 | ultron 0.3 iteration, M2 V1 | 2026-11-27 | 0710, 0711, 0712, 0713, 0716 |
| [0714](0714-show-spec.md) | `show` spec | product, prompts | v1 | ultron 0.3 iteration, M2 R8 | 2026-11-27 |  |
| [0715](0715-brief-retention.md) | Brief retention | product | v1 | ultron 0.3 iteration, M2 R8 | 2026-11-27 |  |
| [0716](0716-jocasta-brief-open.md) | `jocasta brief open` | product | v1 | ultron 0.3 iteration, M2 R8 | 2026-11-27 |  |
| [0717](0717-today-and-ask-layout.md) | `today` and `ask` layout | product | v1 | ultron 0.3 iteration, M2 R8 | 2026-11-27 | 0718 |
| [0718](0718-the-board.md) | The board | product | v1 | ultron 0.3 iteration, M3 W1 | 2026-11-27 |  |
| [0720](0720-benchmark-on-linux.md) | Benchmark on Linux | process | v1 | ultron 0.3 iteration, M3 B1 | 2026-11-27 |  |

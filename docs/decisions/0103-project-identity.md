# 0103 Project identity

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
What makes two folders the same project?

## Facts
Verified 2026-10-07.
- opencode v1.18.22 resolves a project as the normalized origin (lowercased host plus path, `.git` stripped), then a cached id, then the first root commit, then a global bucket for non-git dirs (`packages/core/src/project.ts:96-121`).
- omp v18.6.0 keys sessions by the encoded cwd path only (`docs/session.md:42-47`).
- Microsoft Learn lists the Azure Repos URL forms, with the `visualstudio.com` ones still supported: `https://dev.azure.com/{org}/{project}/_git/{repo}`, `https://{org}@dev.azure.com/{org}/{project}/_git/{repo}`, `https://{org}.visualstudio.com/{project}/_git/{repo}`, `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}` and `vs-ssh.visualstudio.com` (https://learn.microsoft.com/azure/devops/repos/git/clone, https://learn.microsoft.com/azure/devops/repos/git/use-ssh-keys-to-authenticate).
- git 2.54.0 on this machine; a git CLI call took about 60 ms per repo (37 repos under `D:/`, two calls each, 4.4 s).

## Options
| Option | Pros | Cons |
|---|---|---|
| The normalized origin, then the first root commit, then the path | The same repo is one project on any path or machine; old and new Azure DevOps URLs collapse; repos without a remote still get a stable key | A fork whose origin is the fork is its own project; changing the origin changes identity unless rebound |
| The root commit, then the path | Survives remote renames and host moves | A fork and its upstream merge; template repos can share roots |
| The path only | No git calls | A second clone or a moved folder is a new project |

## Recommendation
Origin, then root commit, then path.

## Choice
The normalized `origin` URL (lowercased host plus path, `.git` stripped; every Azure DevOps form maps to `dev.azure.com/<org>/<project>/<repo>`), then the first root commit, then the normalized path. Git is used through its CLI.

## Reason
Not given.

## Consequences
- Key examples, as previewed: `https://github.com/example-org/demo.git` and `git@github.com:example-org/demo.git` → `github.com/example-org/demo`; `https://acme.visualstudio.com/Web/_git/api` and `git@ssh.dev.azure.com:v3/acme/Web/api` → `dev.azure.com/acme/web/api`; no origin → `root:<sha>`; no commits → `path:<path>`.
- The identity key feeds the project id (0107).

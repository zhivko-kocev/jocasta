# 0110 Project display name

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
What name does a project show?

## Facts
- Repos with an origin carry the repo name in their identity key (0103); folder names can differ per clone (0104).

## Options
| Option | Pros | Cons |
|---|---|---|
| The repo name from origin, else the main path's folder name | One name for every clone; matches GitHub and Azure DevOps | Two repos named `api` from different orgs look alike |
| The main path's folder name | What Explorer shows | Changes when the main path moves |

## Recommendation
The repo name, else the folder name.

## Choice
The last segment of the origin key (`demo` for `github.com/example-org/demo`), else the main path's folder name. A rename is a hand edit in `projects.yaml` (0111).

## Reason
Not given.

## Consequences
- The recommendation stored renames through `project rename`; 0111 dropped that command, so renames are hand edits in `projects.yaml`. The entry's format is confirmed with the next questions.

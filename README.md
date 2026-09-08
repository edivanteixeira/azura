# azura

A terminal UI for **Azure DevOps pull requests, pipelines and releases**.
Approving a production deploy becomes one key and one confirmation, instead of five clicks in the portal.

[![ci](https://github.com/edivanteixeira/azura/actions/workflows/ci.yml/badge.svg)](https://github.com/edivanteixeira/azura/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/edivanteixeira/azura)](https://github.com/edivanteixeira/azura/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

![pull requests](assets/prs.svg)

The **blocked on** column answers the question a list of votes cannot: can this
actually merge? It comes from the branch policies evaluated against each PR —
`ready`, `reviewers`, `build`, `strategy`, or `2 checks` when several are pending.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/edivanteixeira/azura/main/install.sh | sh
```

Or grab the binary for your platform from [Releases](https://github.com/edivanteixeira/azura/releases)
— macOS (Intel and Apple Silicon), Linux (x86_64 and arm64), Windows. No runtime, single binary.

```sh
# install somewhere else, no sudo
AZURA_INSTALL_DIR=$HOME/.local/bin curl -fsSL https://raw.githubusercontent.com/edivanteixeira/azura/main/install.sh | sh

# or from source
cargo install --git https://github.com/edivanteixeira/azura
```

## First run

Run `azura`. The first time it asks for your organization, project and a Personal Access Token,
validates them against the API and saves everything to `~/.config/azura/config.toml` with mode `600`.
If you already use `az devops configure`, the organization and project come pre-filled.

Create the PAT under **Azure DevOps → User settings → Personal access tokens**, with these scopes:

| scope | what it unlocks |
|---|---|
| Code (read & write) | read PRs and diffs, vote, complete, abandon |
| Build (read & execute) | list runs and logs, queue, cancel |
| Release (read, write, execute & manage) | approve, reject and promote stages |

To switch project or token later: `azura setup`. `azura --help` lists the flags, `azura --version`
says which build you are running.

## What it does

### Pipelines

`enter` opens the run's steps, `l` jumps straight to the log of the step that failed —
the reason for 90% of trips to the browser.

![pipelines](assets/pipelines.svg)

### Releases

Pending approvals are pinned to the top. Every action that leaves your machine goes through
a confirmation that spells out exactly what is about to happen.

![releases](assets/releases.svg)

### Comments

`t` opens the comment threads of a pull request, `R` replies to one. Threads are
shown with their file context, so a review reads in order.

### PR diffs, inside the CLI

No local clone needed: the changed files and both sides of the diff come from the API.

![diff](assets/diff.svg)

## Keys

`?` opens the full list inside the app.

| | |
|---|---|
| `1` `2` `3` `tab` | PRs · Pipelines · Releases |
| `j/k` `g/G` `ctrl-d/u` | move around |
| `/` `esc` | filter · clear |
| `r` | refresh (automatic every 30s) |
| `o` `enter` | open in browser · detail |
| **PRs** | `a` approve · `w` waiting for author · `x` reject · `c` complete · `D` abandon · `enter` diff · `t` comments |
| **PR scope** | `m` cycles: all → opened by me → waiting for my review · `R` picks a repo |
| **Pipelines** | `enter` steps · `l` log of the failed step · `R` run · `x` cancel · `p` definitions |
| **Releases** | `a` approve · `x` reject · `d` deploy a stage |
| **Diff and log** | `J/K` switch file · `j/k` scroll · `/` `n` search |

The filter matches everything on screen — title, repo, author, branch, state, stages.
`/failed` shows only what broke, `/conflicts` only PRs with merge conflicts, `/production` only production.

`R` narrows the PR list to one repository, listing them by how many open pull requests
each has, so the busy ones are one keystroke away. It stacks with `m` and with `/`.

## Safety

- **`azura --dry-run`** — no writes are sent; every action just describes the request it would make.
  A good way to explore the app risk-free.
- Complete, abandon, queue, cancel, approve, reject and deploy **always** ask for confirmation
  naming the target. Approving a PR (which is reversible) goes straight through.
- Completing a PR sends the merge strategy your branch policy requires. Relying on
  the API default silently breaks the moment a project enables *Require a merge strategy*.
- The token lives in `~/.config/azura/config.toml` with mode `600` and never leaves for any host
  other than your organization's `dev.azure.com` / `vsrm.dev.azure.com`.

## Configuration

```toml
# ~/.config/azura/config.toml
org     = "contoso"
project = "Fabrikam"
pat     = "..."
```

`AZDO_ORG`, `AZDO_PROJECT` and `AZDO_PAT` (or `AZURE_DEVOPS_EXT_PAT`) override the file — handy in
scripts. Instead of a PAT you can also pass a bearer token from the Azure CLI:

```sh
export AZDO_PAT=$(az account get-access-token \
  --scope 499b84ac-1321-427f-aa17-267ca6975798/.default --query accessToken -o tsv)
```

That GUID is not a secret and is not yours: it is Microsoft's public Application ID for Azure DevOps
in Entra ID, identical in every tenant. Azure DevOps has no friendly resource alias, so it is the only
way to tell `az` which resource the token is for. Such a token expires in about an hour, which is why
a PAT is the better default.

## Development

```sh
cargo test                                  # unit tests + every screen rendered
cargo test -- --ignored --nocapture         # smoke tests against your real organization
cargo test shot -- --ignored                # regenerate the README SVGs
```

Two layers of tests, neither needing credentials:

- **Render tests** draw every screen × tab × modal at 4 terminal sizes, including 8×3 —
  that is where layout arithmetic overflows.
- **Screen tests** (`src/e2e.rs`) boot the whole app against a fake HTTP server and read
  the text a person would see: that the status bar is not stuck on `loading…`, that `m`
  cycles the three scopes, that the policy column says what is missing, that a reply
  actually posts — and that no screen slipped back into Portuguese.

The screenshots come out of the real ratatui buffer with fake data, so they cannot drift
away from the actual UI.

## License

MIT

# Guides

Each guide starts from one thing you are trying to do, gives the commands, and
shows real output. Every output was captured by running Tessera 1.2.0 on a
fixture in this repository or a small project built for the guide; the
`indexing …` progress lines, which go to stderr, are left out. Each guide ends
with what the feature does not cover.

New here? Do the [quickstart](../quickstart.md) first. It installs Tessera and
connects your agent.

## What do you want to do?

| I want to… | Guide | Commands |
| --- | --- | --- |
| Connect Tessera to the agent I use | [Connect your agent](connect-your-agent.md) | `init` |
| See who a change affects before I make it | [Check before you edit](check-before-you-edit.md) | `blast`, `context` |
| Find out what an agent broke, and hold it until it is fixed | [Catch what an agent broke](catch-what-an-agent-broke.md) | `verify`, hooks |
| Run only the tests a change can reach | [Run only the affected tests](run-only-affected-tests.md) | `affected` |
| Get a report on every pull request | [Review a pull request](review-a-pull-request.md) | `verify --diff`, the Action |
| Turn the checks on in a repository that already has problems | [Adopt on a legacy repository](adopt-on-a-legacy-repo.md) | `baseline`, `accept` |
| Stop a change to an API that another service still uses | [Block breaking API changes](block-breaking-api-changes.md) | `map`, `verify` |
| Work across a monorepo or several repositories | [Monorepos and several repositories](monorepo-and-several-repos.md) | `map`, `path`, a workspace file |
| Find my way around code I did not write | [Understand a new codebase](understand-a-new-codebase.md) | `find`, `search`, `refs`, `path`, `export` |

Looking for a flag or a format instead? The [reference](../ci.md) pages cover
every option; [troubleshooting](../troubleshooting.md) covers the problems
`tessera status` reports.

# Tessera GitHub Action

Comments on each pull request with what the change breaks — calls to names
that do not exist, wrong argument counts, broken imports, routes or gRPC
methods a client still uses — plus the callers of the changed code and the
tests to run. One comment per pull request, updated on every push.

```yaml
name: Tessera
on: pull_request
permissions:
  contents: read
  pull-requests: write
jobs:
  tessera:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: iamsaquib8/tessera/action@v1
        with:
          fail-on: blocking   # or `never` (default) to only comment
```

| Input | Default | |
| --- | --- | --- |
| `version` | `latest` | Release to install |
| `binary` | | A tessera binary to use instead (for example one built in the job) |
| `base` | the pull request's base | Revision to compare against |
| `fail-on` | `never` | `blocking` fails the job on high-confidence problems |
| `comment` | `true` | Post or update the comment; forks fall back to the job summary |
| `token` | `github.token` | Token used to comment |

Output `blocking` is `true` when the report has blocking problems. The index
is cached between runs, so later runs only re-index changed files.

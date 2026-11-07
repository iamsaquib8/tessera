# Changelog fragments

Each user-facing change adds one file here, so pull requests never edit the
same lines of `CHANGELOG.md`. The maintainer assembles them into the
changelog when a release is cut.

Name the file `<slug>.<kind>.md`, for example `affected-depth-note.added.md`.
The slug is a few words about the change (a pull request number works too).
The kind is one of `added`, `changed`, `deprecated`, `removed`, `fixed`,
`performance` or `security`.

The file holds the entry's text: plain sentences about what a user sees,
without a leading `- ` and without headings. It may span several lines.

```text
`tessera affected` says when its depth limit stopped the walk, with the
number of symbols still used elsewhere; `--depth` raises the limit.
```

Not every change needs one: tests, refactors and CI changes with no effect
on users do not.

```sh
node scripts/changelog.js check                    # validate the fragments
node scripts/changelog.js preview                  # the next release's notes
node scripts/changelog.js release 1.3.0 2026-12-01 # write the section, delete the fragments
```

`release` puts `## 1.3.0 - 2026-12-01` above the previous section, grouped as
Added, Changed, Deprecated, Removed, Fixed, Performance and Security. Add the
release's summary paragraph under the heading by hand, then commit.

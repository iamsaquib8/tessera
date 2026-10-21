# Reddit: r/programming

Audience: broad, sceptical of AI tooling, and quick to remove self-promotion.
Post a link to the dev.to article (`devto-post.md`), not the repository: the
article is a walkthrough with real output, and it links to the repository.
Publish the article first and check that its images render.

## Title

Use the article's title:

`Your coding agent says done. Here is a deterministic way to check`

## Link

The published dev.to URL.

## First comment (as the author, right after posting)

Author here. The short version of the article: coding agents rename a
method, update the file they were looking at, and report success while a
caller elsewhere still uses the old name. Instead of adding a second model
to review the change, Tessera checks it against a static index of the whole
repository: tree-sitter parses, SQLite stores, the same code gives the same
answer every time, and nothing leaves the machine.

A few design choices that are open to argument:

- **It is not a type checker, and does not try to be.** It checks only the
  changed code against the index (missing names, argument counts, broken
  imports, call sites a rename left behind), so it is fast enough to run
  after every edit an agent makes. You should still run `tsc`, `javac` and
  your tests.
- **Confidence is explicit.** Every reference is high, medium, low or
  name-level, and only high-confidence findings block. Against compiler
  ground truth on public TypeScript, JavaScript and Java repositories,
  high-confidence references are right 99.7% of the time. Deep resolution
  is limited to those languages; Python, Go, Rust, C, C++, C#, Ruby and PHP
  resolve within a file and by unique name.
- **Contracts between services are matched statically.** Routes, gRPC
  methods, Kafka topics and tables are read from source in several
  frameworks, so a removed endpoint that another service still calls is
  caught. Anything wired up dynamically is invisible, and Express routers
  mounted under a prefix are not followed yet.

The limits are listed in `docs/when-not-to-use.md` in the repository. I
would like to hear where you think a static check like this stops being
worth it compared with just running the compiler and the tests in the
agent loop.

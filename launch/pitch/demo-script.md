# 60-second demo

One story, three beats: the agent breaks a caller, Tessera stops it, and
the same check runs across services and on the pull request. Every command
and output below was run against the repositories named; rehearse once on
your machine before recording.

## Setup (before recording)

```sh
cargo install tessera-codegraph            # or npm i -g tessera-codegraph
git clone https://github.com/GoogleCloudPlatform/microservices-demo
cd microservices-demo && git checkout 38e7348
tessera index                              # about a second; warms the index
```

For beat 1, use any TypeScript project wired with `tessera init` in Claude
Code, or the fixture in this repository (`tests/fixtures/typescript`).
Terminal at 110×30, font 18 pt, a light theme for projectors.

## Script

| Time | Screen | Say |
| --- | --- | --- |
| 0:00–0:08 | Claude Code, mid-task: "rename `findById` to `findOne`" | "Your agent edits code it has only partly read. It renames a method and says done." |
| 0:08–0:22 | The post-edit hook fires: `[V1] ... findById is not defined ... did you mean UserRepo.findOne?` and the agent fixes the caller before it can stop | "Tessera checked the edit against an index of the whole repository. A caller still uses the old name, so the agent is not allowed to say done until it's fixed. No model involved: tree-sitter and SQLite, same answer every time." |
| 0:22–0:38 | microservices-demo: rename the Go handler `GetQuote`, then `tessera verify --changed` | "It works across services too. This Go handler serves a gRPC method. Rename it, and Tessera finds the two clients, in another service and the frontend, that still call `ShippingService/GetQuote`." |
| 0:38–0:50 | `tessera ui --open`: change impact, then the contract map | "The same data, in a local page: what the change touches, who calls it, which tests to run, and how the services connect." |
| 0:50–0:60 | The pull-request comment from the GitHub Action; then `npx tessera-codegraph init` | "And on every pull request. One command wires it into Claude Code, Codex, Cursor, Gemini and 17 more agents. Local, deterministic, Apache-2.0." |

## Commands for beat 2

```sh
sed -i '' 's/func (s \*server) GetQuote(/func (s *server) GetShippingQuote(/' src/shippingservice/main.go
tessera verify --changed
```

Expected output:

```text
✗ verify: 2 problem(s)
  [V5] src/checkoutservice/main.go:315 `ShippingService/GetQuote` is no longer served: the change removed it from src/shippingservice/main.go, and this call still uses it
  [V5] src/frontend/rpc.go:88 `ShippingService/GetQuote` is no longer served: the change removed it from src/shippingservice/main.go, and this call still uses it
```

Undo with `git checkout src/shippingservice/main.go`.

## If something goes wrong live

- No agent available: run beat 1 as `tessera verify --changed` after a
  manual rename; the finding is the same.
- No network: everything above runs offline once installed.
- Asked "is this an LLM?": no. The index is built by parsers; the numbers
  in the deck come from compiler ground truth.

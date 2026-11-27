# Agents of the repository

Four agents, defined in `.claude/agents/`. The scheduler is the main session: it splits the work, starts the agents and relays what they report. The agents do not talk to each other.

| Agent | Owns | Never touches |
|---|---|---|
| `framework-core` | `core/`, `src/`, generated docs, `CHANGELOG.md`, the look (styles, layout rules, glass) | `examples/`, `finder-rs/` |
| `app-python` | `examples/*.py`, `guides/`, prototypes | `core/`, `src/`, `finder-rs/` |
| `app-rust` | `finder-rs/`, the macOS bundle | `core/`, `src/`, `examples/` |
| `reviewer` | nothing (reads) | everything |

## Flow
1. **Triage.** The scheduler cuts the request into core tasks and app tasks and writes what each app task needs from the core.
2. **Core first.** `framework-core` does its tasks (in sequence when they share files), builds, regenerates the docs and reports the API.
3. **Apps in parallel.** `app-python` and `app-rust` start from that state, each in its own worktree, on the same list of Finder features (the parity list).
4. **Prototypes.** `app-python` may work alone on `examples/proto_*.py`. A missing widget is a file in `docs/requests/` (from `TEMPLATE.md`); the scheduler gives it to `framework-core`.
5. **Review.** `reviewer` checks the branch, then the scheduler asks for the commit (one branch and one pull request per feature; nothing is pushed without the owner of the repository saying so).

## Rules
- One worktree per agent, with its own `target/` and its own `.venv` (a shared `maturin develop` overwrites the extension of the others).
- A change in a file of another agent is a request, not an edit.
- The look of the framework (styles, layout rules) belongs to `framework-core`. If the design work grows, a `framework-style` agent can be cut out of it: same rules, `core/src/style.rs`, `src/style.rs`, `src/layout.rs`, `docs/framework/styling.md` as its files.
- Every report says what was built and run, and what was not verified.

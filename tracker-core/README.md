# Embedded tracker core

Reusable Rust backend for the Rune desktop app. Uses the official PowerSync Native SDK with local-only tables, bundled SQLite and no PowerSync Service connection.

`Backend::start(BackendOptions)` returns immediately and initializes a locked database plus a Tokio runtime on background threads. It automatically resolves GitHub credentials and starts polling unless explicitly disabled. The public snapshot/detail/advice calls send worker commands and wait for a reply: call them from a consumer's background executor, as Rune does through `Latest`, rather than its UI thread. `set_waker` and `revision` expose coalesced database change notifications. `refresh` (one/all repositories) and `refresh_repositories` (an explicit subset) coalesce manual requests and `refresh_status` reports the current job. Call `shutdown` when the consumer exits.

`Store` and `Poller<Source>` can also be used directly from an async Rust application. One mutable GitHub source owns all REST/GraphQL traffic; automatic and manual imports run in the same poller task. Queries and local advice can execute while a network request is pending. Poll checkpoints advance only after all delta pages succeed; discussion snapshots replace the old snapshot only after all discussion sources/pages succeed.

The schema in `src/schema.json` retains compatibility with legacy Node tracker tables. Legacy import reads the old database through a read-only SQLite transaction and writes the new database plus its one-time marker atomically. Source and destination are independent after import. The SDK creates and migrates the destination schema; no Node executable, extension installation, HTTP listener or separate daemon is required.

From the repository root:

```sh
cargo +1.99.1 test --workspace
cargo +1.99.1 run -p tracker-core --bin tracker-inspect -- \
  --data-dir .local/isolated-import --import-db data/tracker.sqlite
```

The inspection command is always offline. Live smoke commands are explicit and require existing GitHub credentials; see the desktop README. All automated tests use isolated databases and fixture HTTP responses, with credential discovery disabled for worker tests.

Repository preferences use `RepositoryManager` and a dedicated `repositories.json`. The worker seeds it once from the old tracker configuration, watches it once a second, and applies valid changes without restart. Invalid edits retain the active configuration and a last-good recovery copy. `repository_settings` returns a revision token; `save_repositories` checks that token again after GitHub-access validation, preventing stale UI drafts from overwriting newer file edits. All access validation shares the poller's paced GitHub client. Removed/disabled repository rows and data remain stored but are hidden from queries and polling.

# Protocol and versioning policy

Everything crossing the boundary between the AgentLens UI and whatever is doing
the observing is a serializable message defined in
[`core/src/protocol.rs`](../core/src/protocol.rs) and mirrored in
[`src/lib/protocol.ts`](../src/lib/protocol.ts). Locally those messages are
passed in-process; over WSL or SSH the same bytes go down a pipe to
`agentlens-daemon`. There is one vocabulary, not two.

`PROTOCOL_VERSION` in `protocol.rs` names that vocabulary's revision. It is
**not** the application version: the app and the daemon can be different
releases as long as they agree here.

## The wire format

One JSON object per line, UTF-8, on the daemon's stdin and stdout.

| Frame      | Direction    | Shape                                                          |
| ---------- | ------------ | -------------------------------------------------------------- |
| `request`  | app → daemon | `{"type":"request","id":N,"command":{"cmd":…}}`                |
| `response` | daemon → app | `{"type":"response","id":N,"result":…}` or `{"…","error":"…"}` |
| `event`    | daemon → app | `{"type":"event","event":"fs-changes","payload":…}`            |

Three invariants the transport depends on:

- **stdout is protocol, exclusively.** A stray `println!` in the daemon
  corrupts the stream unrecoverably.
- **stderr is logs, exclusively.** The app mirrors it and keeps the tail for
  diagnostics, but never parses it.
- **Every frame is exactly one line.** Payloads routinely contain newlines —
  file previews, commit messages — so this is a property of the encoding, and
  it is covered by tests in `daemon/tests/stdio.rs`.

Field names are camelCase throughout, with one exception: fields _inside_ a
command object are snake_case, so the handshake is
`{"cmd":"hello","protocol_version":1}`. `protocolVersion` is accepted as an
alias, so a future version can unify the two spellings without breaking
daemons already in the field.

Requests carry an `id` and may be answered out of order. Ordering between
_concurrent_ requests is the caller's responsibility, as with any async
transport; the app awaits each dependent command before issuing the next.

Closing stdin is how a daemon is asked to shut down. It stops watching and
exits 0.

## The handshake

`Hello { protocol_version }` is the first command on any connection, and the
daemon answers with its own name, version, protocol version, and capability
list. A mismatch is refused there and then, with a message naming both
versions — the alternative is a session that half-works in ways nobody can
diagnose.

`capabilities` is the extension point: a daemon may advertise features a
particular build has, and **the UI must never require anything in it**. An
empty list is a valid, fully functional answer.

## What may change, and when

Within one `PROTOCOL_VERSION`, changes must be **additive only**:

- new commands
- new _optional_ fields on existing commands, responses, or events (`Option<T>`
  in Rust, `#[serde(default)]` on the containing struct)
- new event names
- new variants on an enum **only** where every reader already tolerates unknown
  variants

Anything else bumps `PROTOCOL_VERSION`:

- removing or renaming a command, field, or event
- changing a field's type, or its meaning
- making an optional field required
- changing the units or semantics of an existing value

Practically, that means every protocol struct that a future version might send
a new field on carries `#[serde(default)]`, so an older reader ignores what it
does not understand instead of failing to parse. `Hello` carries it too — a
handshake from a future daemon has to parse far enough for the version check to
produce the guided error rather than "unreadable frame".

## Bumping it

1. Increment `PROTOCOL_VERSION` in `core/src/protocol.rs`.
2. Note it in `CHANGELOG.md` under the release, explicitly — users have to
   reinstall the daemon on every remote machine.
3. Update `docs/REMOTE.md` if the install instructions change with it.

A protocol bump is a breaking change for anyone using a remote workspace, so it
follows the same rule as any other: it belongs in a `MINOR` release at the very
least, and is worth calling out on its own line.

## Keeping the two mirrors honest

`src/lib/protocol.ts` is written by hand, not generated. The costs of a
generator (a build step, a schema layer, generated code in review) were judged
higher than the cost of the discipline, so:

- Every change to `core/src/protocol.rs` changes `src/lib/protocol.ts` in the
  same commit.
- Event names live in constants on both sides — never a string literal at a
  call site — so a rename cannot half-land.
- `core/src/protocol.rs` has round-trip tests over the JSON shapes, and
  `src/lib/protocol.test.ts` asserts the TypeScript side agrees.

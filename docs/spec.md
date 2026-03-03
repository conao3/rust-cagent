# cagent Specification

## Architecture Layers

This project is organized into three layers:

1. `server`
2. `communication`
3. `agent`

Layer dependencies are one-way:

- `server` -> `communication` -> `agent`
- `communication` must not own agent process lifecycle internals.
- `agent` must not depend on Telegram implementation details.

## 1. Server Layer

### Responsibility

The server layer is the process supervisor and entrypoint.

- Starts and keeps `cagent server` running in foreground.
- Exposes internal HTTP endpoints (`/health`, `/spawn`) for child process spawning.
- Starts Telegram communication loop together with API loop.
- Writes server pid to state path.

### In Scope

- Process spawning (`internal claude-wrapper`, `internal codex-wrapper`).
- Child process parent/child relationship management.
- Lifecycle orchestration only.

### Out of Scope

- Telegram message parsing and mapping logic.
- Claude/Codex protocol handling.

## 2. Communication Layer

### Responsibility

The communication layer adapts Telegram I/O to session message channels.

- Receives Telegram updates.
- Derives deterministic session key:
  - `session_id = "{chat_id}:{message_thread_id_or_0}"`
- Sends user text to session input FIFO.
- Reads agent outputs from session output FIFO and sends them back to Telegram.
- Manages conversation-to-session mapping persistence.

### Session Transport Contract

For each session, communication uses named pipes under:

- `/tmp/cagent/session/{session_id}/message_send.fifo`
- `/tmp/cagent/session/{session_id}/message_receive.fifo`

Direction:

- `message_send.fifo`: communication -> agent (user input)
- `message_receive.fifo`: agent -> communication (assistant output events)

### Out of Scope

- PTY handling.
- LLM protocol specifics.

## 3. Agent Layer

### Responsibility

The agent layer encapsulates Claude/Codex runtime behavior.

- Creates and cleans session directories and FIFOs.
- Launches Claude/Codex worker process per session.
- Reads `message_send.fifo` and forwards input to model runtime.
- Emits session messages to `message_receive.fifo`.
- Maintains session metadata and session control functions.

### In Scope

- Claude runtime integration (PTY/session watcher).
- Codex app-server protocol integration.
- Session filesystem layout and lifecycle.

### Out of Scope

- Telegram API operations.
- Server pid/state ownership.

## Session Lifecycle

1. `cagent server` starts.
2. Communication receives Telegram text.
3. Communication derives `session_id` from chat/thread.
4. If session is absent, communication triggers agent launch with that `session_id`.
5. Agent ensures `/tmp/cagent/session/{session_id}` and both FIFOs exist.
6. Communication writes input text to `message_send.fifo`.
7. Agent processes input and writes output events to `message_receive.fifo`.
8. Communication reads events and posts text responses to Telegram.

## Boundary Rules

- `server` coordinates processes, not message semantics.
- `communication` translates external I/O, not model execution internals.
- `agent` executes model sessions, not Telegram transport.
- Shared contract across layers is session id + FIFO path convention.

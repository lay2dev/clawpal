# Remote Doctor Connect Diagnostics Design

**Date:** 2026-03-24

**Goal:** Make Remote Doctor websocket handshake failures actionable by exposing the close/error reason in backend errors, session logs, and Doctor UI messaging.

## Scope

- Preserve the latest websocket disconnect reason inside `NodeClient`.
- Surface that reason in request failures such as `Connection lost while waiting for response`.
- Write Remote Doctor gateway connect failures into the per-session JSONL log.
- Show a more specific Doctor page error when the websocket is accepted but closed before the server replies.

## Non-Goals

- No protocol changes to clawpal-server.
- No new persistent settings.
- No large telemetry/event schema redesign.

## Approach Options

### Option 1: Session log only

Add a `gateway_connect_failed` event to the Remote Doctor session log and keep the current UI text.

**Pros:** Smallest code change.
**Cons:** Users still see the same vague error until someone opens logs.

### Option 2: Backend diagnostics plus UI hint

Capture the latest websocket close/error reason in `NodeClient`, include it in handshake/request errors, write the failure to the Remote Doctor session log, and map the Doctor UI to a more actionable message.

**Pros:** Best balance of debuggability and user clarity.
**Cons:** Slightly broader surface area.

### Option 3: Full structured websocket tracing

Emit challenge/connect/close frames and handshake state transitions as dedicated debug events.

**Pros:** Richest diagnostics.
**Cons:** Too much scope for the current need.

## Recommended Design

Use **Option 2**.

`NodeClient` should keep an in-memory `last_disconnect_reason` string. When the websocket reader receives a close frame, it should record a message with the close code and optional reason. When it receives a websocket transport error, it should record that error text. Any pending `send_request()` call that loses its response channel should include this stored reason in the returned error string.

Remote Doctor should log gateway connect failures immediately after `client.connect(...)` fails. The log entry should include the session id, gateway URL, whether an auth token override was present, and the specific error text. This makes the JSONL artifact useful even when the connection fails before any plan request is sent.

The Doctor page should turn the raw `Connection lost while waiting for response: ...` family into a more actionable message that tells the user the websocket was accepted but the server closed before replying, and that the invite-code-derived token or saved Remote Doctor auth token should be checked first. The original low-level detail should still be kept in the displayed message.

## Testing

- Add Rust unit tests for the disconnect-reason formatting helper(s).
- Add a Rust unit test for the session log helper that writes gateway connect failures.
- Add a frontend unit test for the Doctor-facing error formatter.

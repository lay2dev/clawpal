# ClawPal Server URL Default Update Design

**Date:** 2026-03-24

**Goal:** Update ClawPal's default clawpal-server endpoints from `127.0.0.1:3000` to `65.21.45.43:3040` without changing how user-saved overrides work.

## Scope

- Update the default Remote Doctor websocket gateway URL to `ws://65.21.45.43:3040/ws`.
- Update the default invite-code exchange base URL to `http://65.21.45.43:3040`.
- Update frontend fallback logic, UI placeholder text, and related tests so the app surface stays consistent.

## Non-Goals

- No new settings fields.
- No runtime detection or environment-based switching.
- No refactor to shared cross-language constants in this change.

## Approach Options

### Option 1: Backend-only hardcoded update

Change only the Rust defaults used by Remote Doctor and invite exchange.

**Pros:** Smallest code diff.
**Cons:** Frontend placeholders, fallback URL derivation, and tests would still point at the old address.

### Option 2: Unified default update across backend and frontend

Change every default/fallback/reference that currently treats `127.0.0.1:3000` as the clawpal-server default.

**Pros:** UI text, fallback behavior, logs, and tests all stay aligned.
**Cons:** Slightly broader edit set.

### Option 3: Shared config abstraction

Introduce a shared constant/config layer for the default URL family.

**Pros:** Cleaner long-term maintenance.
**Cons:** Unnecessary refactor for a one-address change.

## Recommended Design

Use **Option 2**.

The Rust Remote Doctor config should keep ignoring any saved gateway URL override for the current fixed server path behavior, but the fixed websocket constant should move to `ws://65.21.45.43:3040/ws`.

The invite exchange flow should move its fixed HTTP endpoint to `http://65.21.45.43:3040/api-keys/exchange`, and the frontend fallback helper should derive `http://65.21.45.43:3040` whenever no gateway URL is provided or parsing fails.

Settings copy and placeholder text should show the new websocket endpoint so users see the same default the app actually uses. Any logging payloads that embed the old default URL should be updated too.

## Data Flow

1. Remote Doctor repair loads the fixed gateway URL from Rust config.
2. Invite-code exchange posts to the fixed HTTP exchange endpoint in Rust.
3. Frontend fallback logic uses the new HTTP base URL when the gateway URL is blank or invalid.
4. Settings screen examples and logging reflect the same websocket default.

## Error Handling

- Existing blank-input and invalid invite-code handling stays unchanged.
- Invalid custom gateway URLs in the frontend should continue to fall back to the default HTTP base URL, now pointing at `65.21.45.43:3040`.

## Testing

- Update the Rust unit test that asserts the fixed Remote Doctor gateway URL.
- Update the frontend invite-code tests to assert the new default HTTP base URL and exchange endpoint.
- Run the focused frontend and Rust tests that cover the changed defaults.

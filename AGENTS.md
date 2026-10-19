# Oddity

## Project

Oddity is a Linux launcher for *Neverness To Everness* (NTE).

- **Platform:** Linux x86_64, Wayland and X11.
- **UI:** Tauri 2, WebKitGTK 4.1, native window.
- **Runtime:** `umu-run` with Proton.
- **Scope:** Replaces the launcher, not the game itself.

### Architecture

- `nte-core`: Shared business logic, independent of Tauri and CLI.
- `nte-cli`: Command-line interface built on `nte-core`.
- `nte-app`: Tauri application, UI and process lifecycle.
- `launcher/ui`: Plain HTML, CSS and ES modules.

Prefer reusable logic in `nte-core` over duplication.

## Working Principles

- Understand the relevant code before making changes.
- Focus on the intended outcome rather than following a rigid procedure.
- Prefer simple, maintainable solutions consistent with the existing architecture.
- Comments should be concise, natural Chinese and explain intent rather than restating code.
- User-facing text should use formal Chinese with consistent terminology and ASCII punctuation.
- Use judgment when adding tests. Test meaningful behavior, not implementation details.
- Verify relevant changes when practical, and report anything left unverified.

## Decision Management

**The current decision is the only authoritative decision.**

- Describe the current design directly. Do not narrate previous decisions, rejected alternatives, reversals, or decisions not taken.
- Do not introduce regression tests merely to demonstrate that an abandoned decision has not been restored.
- When a decision changes, remove its superseded implementation, configuration, documentation, comments, and obsolete tests.
- Do not retain legacy branches, compatibility layers, fallback paths, or historical explanations solely to preserve an abandoned decision.

## Completion

Briefly report what was implemented, what was verified, and any remaining issues.
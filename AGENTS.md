# Oddity

## Project

Oddity is a Linux launcher for *Neverness To Everness* (NTE).

- **Platform:** Linux x86_64, Wayland and X11.
- **UI:** Electron 44, Chromium, native window.
- **Runtime:** `umu-run` with Proton.
- **Scope:** Replaces the launcher, not the game itself.

### Architecture

- `nte-core`: Shared business logic, independent of Electron and CLI.
- `nte-cli`: Command-line interface built on `nte-core`.
- `nte-service`: Rust application state, commands and process lifecycle.
- `launcher/ui/electron`: Electron main process and isolated preload.
- `launcher/ui`: Vite + TypeScript + Vue 3 frontend.

Prefer reusable logic in `nte-core` over duplication.

## Working Principles

- Understand the relevant code before making changes.
- Focus on the intended outcome rather than following a rigid procedure.
- Prefer simple, maintainable solutions consistent with the existing architecture.
- Comments should be concise, natural Chinese and explain intent rather than restating code.
- User-facing text should use formal Chinese with consistent terminology and ASCII punctuation.
- Use judgment when adding tests. Test meaningful behavior, not implementation details.
- Verify relevant changes when practical, and report anything left unverified.

## Technical Research

When encountering unfamiliar functionality, do not immediately implement it from scratch. First search GitHub for similar open-source projects that are actively maintained and have clear licensing, and compare their implementation approaches, dependencies, and known issues. Prefer proven designs; do not directly copy incompatible code or code of unknown origin.

## Branches and PRs

- Before creating a branch or submitting a PR, confirm that your branch is based on the latest `origin/main`.
- Do not use PRs as a dumping ground for irrelevant remarks that are not meant for maintainers.

## Decision Management

**The current decision is the only authoritative decision.**

- Describe the current design directly. Do not narrate previous decisions, rejected alternatives, reversals, or decisions not taken.
- Do not introduce regression tests merely to demonstrate that an abandoned decision has not been restored.
- When a decision changes, remove its superseded implementation, configuration, documentation, comments, and obsolete tests.
- Do not retain legacy branches, compatibility layers, fallback paths, or historical explanations solely to preserve an abandoned decision.

## Completion

Briefly report what was implemented, what was verified, and any remaining issues.

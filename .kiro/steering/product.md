# Product: DiskCleaner

An open-source, Windows-first disk analyzer and cleanup advisor (WizTree-like speed, plus relationship mapping and an optional AI explanation layer). Public repo, MIT licensed, portfolio-quality.

## Goals
1. Free disk space
2. Find leftovers from uninstalled apps
3. Show what is using space, and why (app -> executables -> folders -> files)

## Current scope: BACKEND ONLY
- Rust workspace under `crates/`. No frontend code, no Tauri, no React. A later phase will wrap `dc-api` in a UI.
- Follow `ROADMAP.md` at the repo root. Work one phase (one spec) at a time. Do not start a later phase early.
- Do not add features that are not in the current spec's tasks. If you think something is missing, say so in your summary instead of building it.

## Users and tone
- Primary user is the developer (experienced in logic, reviews rather than writes syntax). Explain design choices briefly in plain language when you finish a task; keep code idiomatic and well-commented where non-obvious.
- Public users will trust this tool with their files. Correctness and safety beat speed and features.

## Non-goals
- No telemetry, no analytics, no network calls except the opt-in advisor providers.
- No registry or file modification of any kind before Phase B9.
- No process killing, no service control, no installers/uninstallers invoked.

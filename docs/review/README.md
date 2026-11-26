# Reviewing Rhizome: every feature, where it is, how to check it

The owner asked (2026-11-26) that every feature, step and procedure can be reviewed when the
project is done. This page is the map: for each feature, what the person sees, where the code and
the written reasons are, which tests prove it, how to check it by hand, and whether it is on by
default. The safety rules are in `../../SAFETY.md` and `../../CLAUDE.md`; every decision is a
`D` number in `../DECISIONS.md`; things noticed along the way are `N` numbers in `../NOTES.md`.

## 1. How to review (in this order)

1. Read `SAFETY.md` (11 rules) and `CLAUDE.md` (hard rules 1 to 12).
2. Run every check (section 4). All must pass; none may be skipped or weakened.
3. Go through section 2 feature by feature: read the decision, open the code, run the test, do
   the "by hand" step.
4. Go through section 3 (the parts that change the disk) with extra care.
5. Read the open items (section 6).

## 2. Features

Default: **on** unless it says otherwise. "Desktop" = the Windows app; "browser" = the demo page.

### Seeing what uses the space

| Feature | Person sees | Code | Reasons | Tests | By hand |
|---|---|---|---|---|---|
| Scan (read-only) | Pick a folder or drive; it fills in live | `crates/dc-scan`, `dc-api/src/live.rs`, `src-tauri` `start_scan` | phases b1, f3; D95-D98 | `cargo test -p dc-scan` (includes the before/after hash test: nothing changes) | Scan a folder, compare sizes with Explorer's Properties |
| Windows facts | App names, publishers, last used, icons; games from Steam, Epic, Riot, HoYoPlay, GOG, Ubisoft | `crates/dc-winmeta` | phase b2; D124, D125, D128, D142 | `cargo test -p dc-winmeta` (fixtures, `FakeSource`) | Library: names and "used X ago" look right |
| Who owns what | Each folder's app | `crates/dc-attrib` | phase b3 | `cargo test -p dc-attrib` | An app's page lists its places |
| Protected list | Windows and other places are never offered | `crates/dc-protected` | phase b4a | `cargo test -p dc-protected` | Try to tick `C:\Windows`: refused |
| Rules (≈168) | Safe / Probably safe / Don't touch with reasons | `rules/junk/builtin.toml`, `crates/dc-rules` | phase b4b; D129 | `cargo run -q -p dc-cli -- rules --check rules/junk/builtin.toml` | Findings tab: each has evidence |
| Leftovers | Folders of uninstalled apps | `crates/dc-leftovers` | phase b5 | `cargo test -p dc-leftovers` | Findings: "leftover" items name the app |
| Home (calm, first screen) | How full, four tasks, biggest things, can-free card | `apps/desktop/src/panels/home.ts` | D140 | `tests/unit/home.test.ts`; browser checks "Home …" | Open the app: Home, light calm look, no side panel |
| Map | Folders as circles | `apps/desktop/src/graph` | phases f1, f2 | browser checks (map section) | Map tab; **concept to be replaced later (owner)** |
| Files list | Biggest first, sort, details | `panels/explorer.ts` | f2 | browser checks | Files tab |
| Overview ("All the numbers") | Totals, downloads, big/old files, duplicates | `panels/overview.ts`, `core/inventory.ts`, `core/dupes.ts` | phase f4 | browser checks "Overview …" | Home → All the numbers |
| Library | Apps by kind, your files by type, launchers and their games | `panels/apps.ts`, `core/kinds.ts`, `core/yours.ts` | D132 | `tests/unit/library.test.ts`; browser checks | Library: move an app to another kind (right-click), it stays |
| Details panel | Simple first; **Show details** switch (off by default, remembered) | `panels/panels.ts`, `core/explain.ts` (`canDelete`) | D136, D140 | `tests/unit/explain.test.ts`, `home.test.ts`; browser checks | Files → click a folder → "Can I delete it?" |
| "What is this?" (≈95 places) | What it is, if deleted, how to free | `core/explain.ts` | D136, N89 | `tests/unit/explain.test.ts` | Details of `AppData\Local\Temp` |
| Preview window | Play videos and music, view pictures, PDFs, text (read-only) | `src-tauri/src/viewer.rs`, `panels/viewer.ts`, `core/view.ts` | D143 | `viewer.rs` unit tests; browser check "preview" | Library → Videos → ▶ Play all; a file's details → Play |
| Same-named apps | "web app in Brave", or publisher/place | `core/kinds.ts` `webAppOf`, `panels/apps.ts` | D144 | `library.test.ts` | Library: the two Spotify cards |
| Search (no AI) | Names, paths, apps, kinds of your files | `main.ts` `runSearch`, `searchLibrary`; `core/search.ts` | D140 | browser check "search finds apps" | Type `steam`, then `screenshots` |
| Your AI (MCP), opt-in, **off by default** | Settings → let my AI read my latest scan | `dc-api/src/mcp.rs`, `datadir.rs` | D-series in "Your own AI" (CHANGELOG) | `cargo test -p dc-api mcp` | Settings: off by default; the log shows what was asked |
| Scan report | Kept in Rhizome's data folder (on by default, Settings) | `dc-api/src/report.rs` | CHANGELOG | `cargo test -p dc-api report` | Settings → Copy the report |
| Advisor (backend only) | Not in the app yet | `crates/dc-advisor` | phase b8 | `cargo test -p dc-advisor` | — |
| Snapshots and diffs | Not in the app yet | `crates/dc-store` | phase b6 | `cargo test -p dc-store` | — |
| Fast NTFS scan | **Not built** (B7 needs `docs/mft-design.md`) | `crates/dc-mft` (protocol only) | phase b7 | — | — |

### Changing the disk (desktop only)

| Feature | Person sees | Code | Reasons | Tests | By hand | Default |
|---|---|---|---|---|---|---|
| Removing | Tick → review → moved to a holding folder | `dc-quarantine/src/engine.rs`, `dc-api/src/cleanup.rs`, `main.ts` review | phase b9 design; D119, D130, D131, D135 | `cargo test -p dc-quarantine --features destructive-actions` (30 + unit), browser checks | Throwaway folder test (N88) | on (desktop) |
| Free up space (guided) | Step 1 caches, 2 unused apps, 3 games | `panels/free.ts` | D140 | browser checks "Free up space …" | Home → Free up space → Tick all safe → review | on |
| Removed tab | Put back, delete for good, dates | `main.ts` `renderRemoved` | CHANGELOG (Removed tab) | browser checks | Remove something, put it back | on (desktop) |
| Retention | 7 / 14 / 30 / 60 days | Settings, `engine.rs` `empty_expired` | D131 | engine tests | Settings → 30 days (default) | 30 days |
| Uninstalling | The app's own uninstaller, then what it left behind | `dc-quarantine/src/uninstall.rs`, `src-tauri` `uninstall_app` | D133, D134 | `uninstall.rs` tests | Uninstall a small app (N85) | on (desktop) |
| **Moving an app** | Move to another drive → It works / Move it back | `dc-quarantine/src/mover.rs`, `dc-api/src/moving.rs`, `src-tauri/src/moving.rs`, `panels/move.ts` | `docs/phases/b11-move-apps/` (R1-R27, design); D141 | `tests/mover.rs` (16), unit tests, browser checks "move …" | design section 10 test plan | **off** (`--features moving`) |

### Making releases

| Feature | Code | Reasons | Check |
|---|---|---|---|
| Release build on GitHub | `.github/workflows/release.yml` (**the owner must still save it into the repo**) | D137, phase R1 | A draft release with checksums |
| Code signing | `docs/code-signing-policy.md` | D137 | SignPath application (later) |
| Name and publisher | `tauri.conf.json` | D138 | Installed apps shows "Rhizome - Disk Space Manager", Randel Jhon L Serafica |

## 3. The parts that change the disk (extra care)

Only `crates/dc-quarantine` may change files (a Clippy rule fails the build otherwise). Check:

- `engine.rs`: removing only renames into a holding folder on the same drive; deleting only
  inside a holding folder; the journal is written before each step.
- `mover.rs` (B11): the order in `design.md` section 2; copies deleted only through `discard`
  (renamed into the holding folder first); the original deleted only by `confirm` or recovery's
  `finish_confirm`, inside the holding folder; the junction removed only after `our_link`.
- `ops.rs`: the only calls to rename, copy, remove; `MoveOps` separate from `Ops`.
- `win.rs`: every `unsafe` block has a `SAFETY` note; handles closed once; buffers sized right
  (`make_junction`'s reparse buffer is built in safe code, `mover::mount_point_buffer`, and read
  back by `parse_mount_point`, both unit-tested).
- `uninstall.rs`: only an app's own recorded uninstaller, never a shell.
- The desktop commands re-decide everything from the latest scan and Windows' list; the page
  only says what the person picked.

## 4. Every check

From the repository root. On Windows all of it is one command:
`powershell -ExecutionPolicy Bypass -File tools\check-all.ps1` (prints ALL CHECKS PASSED).

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features dc-desktop/moving -- -D warnings
cargo test --workspace
cargo test -p dc-quarantine --features destructive-actions
cargo deny check
cargo run -q -p dc-cli -- rules --check rules/junk/builtin.toml
cd apps/desktop
npm run -s check
npx vitest run
npm run -s build && npx vite preview --port 4173   (in a second window)
npm run -s e2e          (prints ALL PASSED)
```

On Windows also: `cargo build -p dc-desktop --features moving` and the same clippy line (the
build container cannot compile for Windows; N91, N93).

## 5. Defaults (what a new install starts with)

| Setting | Default |
|---|---|
| Look | Calm, light (Settings has the other looks) |
| First view | Home |
| Details panel | Simple (Show details off) |
| Size units | Windows' (GB as Windows counts) |
| Show when apps were last used | On |
| Keep a report of my latest scan | On |
| Let my AI read my latest scan | **Off** |
| Removed items kept | 30 days |
| Removing and uninstalling | On in the desktop app |
| Moving apps | **Off** (build switch `moving`) |

## 6. Open items (as of 2026-11-26)

- The owner's side: commit the redesign (D140) and B11 (D141); save `release.yml`; Defender full
  scan and VirusTotal (N90); rescan to confirm Genshin ≈174 GB.
- B11: compile on Windows; run the test plan (design section 10); then switch `moving` on.
  Program Files apps need an elevated helper (owner's decision).
- B7 fast scan; the new map concept; more launchers (EA, Battle.net records); confirm GOG and Ubisoft (N94).
- `docs/review/worklog-2026-11-26.md` lists everything done in the night session of
  2026-11-26, step by step.

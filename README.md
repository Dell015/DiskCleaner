# Rhizome

**Rhizome - Disk Space Manager.** See and clean your disk: an open-source, Windows-first disk space manager.

Rhizome reads your drive and shows where the space goes: **which app owns which files and why**,
what uninstalled apps left behind, which downloads you no longer need, and which files are
duplicates. It explains every finding in plain words, and it never changes anything you did not
pick and confirm yourself.

> Rhizome was called **DiskCleaner** until November 2026. Some internal names (the `dc-…` crates,
> the `dc` command, the data folder) still use the old name.

> **Status: early development.** Scanning, the desktop app and the AI connection work. Removing
> files and uninstalling apps are on in the desktop app since November 2026, after testing on a
> real PC. Nothing changes unless you pick it and confirm it, and removed things can be put back
> for 30 days. The command line and the AI connection stay **read-only**. See
> [SAFETY.md](SAFETY.md).

## What it does

- **Overview:** where the space goes (apps, your files, Windows, everything else); every installed
  app with its size, places, install date and when you last used it; programs nobody installed;
  your Downloads sorted by type with hints (installers of apps you already have, archives you
  already unpacked, files untouched for a year); the biggest and oldest files.
- **Library:** every app grouped by kind (games, launchers, developer tools, creative, browsers,
  chat, office, AI, players, hardware, security, utilities, system parts). A launcher lists the
  games it installed, and each game says where it came from; you can move an app to another kind.
  **Your files** are grouped by what they are (videos, pictures, music, documents, code projects,
  downloads, archives), then by kind (screenshots, camera photos, recordings, PDFs, ...).
- **Files:** a sortable folder list with size bars, owners and verdicts. Click anything to see
  **what it is for, what happens if you delete it, and the right way to free its space**. "Sort this folder" tidies
  the view of any messy folder or drive.
- **Map:** the whole disk as a graph of nodes you can drag and zoom, coloured by risk, app or type.
- **Leftovers:** folders that uninstalled apps left behind, with the evidence and how sure it is.
- **Duplicates:** exact copies found by reading the files, biggest first, gentle on hard disks.
- **Scan report:** every finished scan writes a plain report for you (and for an AI, if you want).
- **Your own AI as advisor:** a read-only [MCP](https://modelcontextprotocol.io) server lets
  Claude or any MCP app answer "is this safe to delete?", "which apps don't I use?" or "where are
  my projects?" from your latest scan. Off until you switch it on; every question is logged.
- **Removing safely:** picked items are moved to a holding folder on the same drive,
  can be restored for 30 days, and are only then deleted for good.
- **Uninstalling:** from an app's Library page, Rhizome starts the app's own
  uninstaller (never silently, never through a command prompt). A launcher is guided in order: its
  games and anti-cheat first, the launcher last. Afterwards Rhizome checks just that app's folders
  and lists what it left behind, to remove in the same place.

## Safety first

- Default-deny: anything Rhizome is unsure about is shown, never offered.
- Windows, program folders, app data, browser profiles, wallets and a compiled-in protected list
  can never be removed. Links and junctions are never followed.
- Only metadata (names, sizes, dates) ever goes to an AI, and only if you switch it on.
- Tests never touch real files.

The full rules are in [SAFETY.md](SAFETY.md).

## Run the desktop app

You need [Rust](https://rustup.rs), Node.js 20.19 or newer, and on Windows the Visual Studio C++
Build Tools and the WebView2 runtime (part of Windows 11).

```powershell
cd apps/desktop
npm install
npm run tauri dev     # the first build takes several minutes; then a window opens
```

Pick a folder or drive and watch it fill in while the scan runs. More in
[apps/desktop/README.md](apps/desktop/README.md), including a browser demo that needs no Rust.

## Command line (read-only)

```
cargo run -p dc-cli -- scan <folder> --out scan.json
cargo run -p dc-cli -- top scan.json --dirs
cargo run -p dc-cli -- report <view file>
cargo run -p dc-cli -- mcp
```

The scan file must not exist yet; nothing in the scanned folder is ever changed.

## Developing

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

In `apps/desktop`: `npm test` (unit tests), `npm run build` (type check and build),
`npm run e2e` (browser checks).

### Dummy test data

Tests and experiments must never touch real files. Generate a deterministic fake tree:

```
cargo run -p fixture-gen -- --out fixtures\sample
```

The output folder must be new or empty. The generator only creates files; it never deletes.

## Project layout

```
crates/         Rust crates (scanner, rules, leftovers, removal engine, CLI `dc`, ...), one folder each
rules/          rule packs as data: junk/ cleanup rules, apps/ well-known apps
apps/desktop/   the desktop app (Tauri + TypeScript): Overview, Files and Map
tools/          test helpers (fixture-gen)
docs/           architecture, decisions, notes, per-phase plans (start at docs/README.md)
data/           your own scans and view files; ignored by git
```

See [docs/architecture.md](docs/architecture.md), [ROADMAP.md](ROADMAP.md) and
[CHANGELOG.md](CHANGELOG.md).

## License

MIT. See [LICENSE](LICENSE).

## If Windows warns you

Rhizome is new, and its downloads are not signed yet (that is being set up: see the
[code signing policy](docs/code-signing-policy.md)). So Windows may show "Windows protected your PC",
or an antivirus may flag the program. That is common for new, unsigned programs, especially ones
that can move and delete files, as Rhizome can when you ask it to.

Before running a download, you can check it is the real thing:

1. Download only from this repository's [Releases](https://github.com/Dell015/DiskCleaner/releases) page.
2. Compare its checksum with `SHA256SUMS.txt` from the same release:
   `Get-FileHash <file> -Algorithm SHA256` in PowerShell.
3. Check it was built by GitHub from this code: `gh attestation verify <file> --repo Dell015/DiskCleaner`.
4. Each release links a VirusTotal report, where about 70 antivirus programs have checked it.

If all of that matches, choose "More info" → "Run anyway" on the Windows warning. If anything does
not match, do not run it, and please open an issue.

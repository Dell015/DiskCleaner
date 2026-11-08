# Phase B2: Windows metadata - Design

Status: **draft for review**. Requirements: `requirements.md`.

## 1. Crate layout
`dc-winmeta` keeps the portable model and logic in plain modules and puts everything that talks to Windows behind `#[cfg(windows)]`.

```
dc-winmeta/src
  lib.rs
  probe.rs          (B1, Windows only)
  model.rs          InstalledApp, KnownFolder, Shortcut, VersionInfo, SignatureStatus, AppSnapshot
  source.rs         trait SystemInfoSource, FakeSource
  parse.rs          pure parsers: icon paths, uninstall strings, env expansion, folder derivation
  lnk.rs            safe .lnk parser (MS-SHLLINK subset)
  apps.rs           build_snapshot(source, options): merging rules, flags, stale detection
  windows/
    mod.rs          WindowsSource
    registry.rs     read-only registry enumeration
    folders.rs      SHGetKnownFolderPath
    version.rs      GetFileVersionInfo* wrappers
    signature.rs    WinVerifyTrust wrapper (B2c)
```
`dc-cli` adds `apps_cmd.rs` and the `--apps` option in `scan_cmd.rs`. `dc-core` is unchanged.

## 2. The trait
```rust
pub trait SystemInfoSource {
    fn known_folders(&self) -> Vec<KnownFolder>;
    fn uninstall_entries(&self) -> SourceResult<Vec<RawUninstallEntry>>;
    fn shortcuts(&self) -> SourceResult<Vec<Shortcut>>;
    fn version_info(&self, exe: &Path) -> Option<VersionInfo>;
    fn signature(&self, exe: &Path) -> SignatureStatus;
}
```
`SourceResult` carries the data plus a list of non-fatal problems (key unreadable, shortcut broken). `build_snapshot` is pure: given a source and options it produces the `AppSnapshot`, so nearly all logic is tested with `FakeSource` on Linux.

## 3. Data model (serialized, `schema_version = 1`)
- `InstalledApp`: `key`, `source` (`machine64|machine32|user`), `name`, `publisher`, `version`, `install_folder` (+ `folder_origin`), `display_icon`, `uninstall_command` (text only), `estimated_size_kb`, `install_date`, `flags` (`system_component`, `update_or_patch`, `windows_installer`, `hidden_entry`, `stale_registration`, `no_install_folder`), `main_exe`, `version_info`, `signature`, `shortcuts`.
- `AppSnapshot`: `schema_version`, `created_by`, `known_folders`, `apps`, `problems`.
Paths are stored as text exactly as derived; case-insensitive comparison happens in code, not in the data.

## 4. Registry (read-only)
- Use `windows-sys` `Win32_System_Registry`. Only `RegOpenKeyExW` with `KEY_READ | KEY_WOW64_64KEY / KEY_WOW64_32KEY`, `RegEnumKeyExW`, `RegQueryValueExW`, `RegCloseKey`. Handles are wrapped in a small owning type that closes them on drop.
- Values read as `REG_SZ` or `REG_EXPAND_SZ` (expanded by our own safe expansion), `REG_DWORD` for numbers. Other types are ignored. Strings are length-checked and lossy-decoded.
- A tripwire test greps the crate sources for `RegSetValue`, `RegCreateKey`, `RegDelete`, `RegSaveKey`, `Command::new` and `ShellExecute` and fails if found (outside this design doc and the test itself).

## 5. Install-folder derivation (pure, `parse.rs`)
Order: `InstallLocation` -> folder of the exe in `DisplayIcon` (strip `,index`, quotes, args) -> folder of the exe in `UninstallString` (handle quoted paths, `MsiExec.exe /X{GUID}` which has no folder, and `rundll32`). Results are normalized (expand env vars, trim quotes and trailing separators). A candidate that is a bare drive root or a known system folder (Windows, System32, Program Files itself) is rejected as too broad. Stale = derived folder does not exist (checked with `symlink_metadata`, read-only).

## 6. Known folders
`SHGetKnownFolderPath` with the KNOWNFOLDERID constants, result freed with `CoTaskMemFree`. This needs `Win32_UI_Shell` and `Win32_System_Com`. Each call is one small `unsafe` block with a `SAFETY` comment.

## 7. Shortcuts
`.lnk` files are read as bytes and parsed in safe Rust: header (size 0x4C, CLSID check), link-target ID list skipped, `LinkInfo` local base path + common suffix, then string data (relative path, working dir, arguments). Unicode variants preferred. Bounds-checked everywhere; any inconsistency returns `None`. This avoids COM (`IShellLink`), so there is no extra `unsafe` and the parser is fuzz-tested with arbitrary bytes on Linux.

## 8. Version info
`GetFileVersionInfoSizeW`, `GetFileVersionInfoW`, `VerQueryValueW` (feature `Win32_Storage_FileSystem`; links `version.lib`). These read the file's version resource without running it. The translation table picks the language/code page; the four string fields are read and sanitized. Failure returns `None`.

## 9. Signatures (step B2c)
`WinVerifyTrust` with `WINTRUST_ACTION_GENERIC_VERIFY_V2`, `WTD_UI_NONE`, `WTD_REVOKE_NONE`, `WTD_CACHE_ONLY_URL_RETRIEVAL`, `WTD_SAFER_FLAG` off, and state data closed afterwards with `WTD_STATEACTION_CLOSE`. Signer name via `WTHelperProvDataFromStateData` / `CertGetNameStringW`. Mapping: success -> `signed(signer)`, `TRUST_E_NOSIGNATURE` -> `unsigned`, other trust errors -> `invalid(code)`, API failure -> `unknown`. Catalog-signed system files are reported `unknown` in B2 (limitation). This module is the largest `unsafe` surface; it has its own review pass and is isolated so it can be dropped or deferred without touching the rest.

## 10. CLI
- `dc apps`: builds the snapshot from `WindowsSource`, prints a table sorted by name, hides `hidden_entry`/`update_or_patch` unless `--include-hidden`; `--out` writes JSON with `create_new`. On non-Windows it exits with a clear message.
- `dc scan ... --apps [FILE]`: after the scan, for each directory node find the app whose `install_folder` is the longest path-prefix match (case-insensitive, whole path components) and store its name in the output as an `app` label list in `ScanResult` (additive field, `schema_version` bumped). Totals are unchanged.
- Failures in any source become `problems` in the snapshot and exit code 2.

## 11. Sub-steps (each ends with fmt, clippy, tests and your check)
| Step | Content |
|---|---|
| B2a | model, trait, FakeSource, parsers, registry source, known folders, `dc apps`, snapshot |
| B2b | `.lnk` parser, shortcut linking, version info, stale flags, `dc scan --apps` |
| B2c | signatures and `--verify-signatures` |

## 12. Risks
1. Registry data is messy (broken quoting, missing fields). Mitigation: pure parsers with property tests, problems list instead of failures.
2. Two apps sharing a folder or one app with several entries. Mitigation: no merging in B2; longest-prefix match only; B3 handles the graph.
3. `.lnk` format edge cases. Mitigation: strict bounds checks, skip on doubt.
4. Signature API complexity and catalog-signed files. Mitigation: separate step B2c, documented limitation.
5. Windows-only code cannot be compiled in the build container. Mitigation: keep Windows modules thin, put logic in portable modules, verify on your machine and CI.

## 13. Decisions recorded
Per-user and machine-wide, classic only, shortcuts yes, version info yes, signatures opt-in/offline/last, `dc apps` plus `dc scan --apps`, stale flag yes, snapshot yes, Windows-side verification yes.

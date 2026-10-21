# SAFETY RULES (always in effect; never weaken)

This tool will inspect people's entire disks and advise on deletion. A bug here can destroy data. These rules override any spec task, user shortcut request, or convenience.

## Hard rules
1. **Read-only until Phase B9.** Before B9, no code path may delete, move, rename, truncate, or write to any scanned path, and no registry writes. Open files read-only with shared access. Add/maintain a test that hashes a fixture tree before and after every scan and asserts nothing changed.
2. **No destructive APIs outside `dc-quarantine`.** `remove_file`, `remove_dir*`, `fs::rename`, `DeleteFileW`, `MoveFileEx`, `SetFileAttributes` (write), `RegSetValue`, `RegDeleteKey`, etc. must not appear anywhere else. If a task seems to require one, STOP and ask.
3. **Default-deny.** Anything unclassified is `DontTouch`.
4. **Risk ordering:** `Safe < ProbablySafe < DontTouch`. Final risk = `max(rule_risk, advisor_risk)`. The advisor can only raise caution. A rule that says `DontTouch` is final and the advisor is not even consulted for it.
5. **Protected paths are compiled in** and checked after canonicalization (case-insensitive, `\\?\` stripped, `..` resolved, 8.3 short names expanded, junctions/symlinks resolved, trailing dots/spaces handled, UNC and device paths handled). Rule packs cannot override them. Any rule that would classify a protected path below `DontTouch` is a validation error.
6. **Metadata only to AI.** The advisor input type contains no file-content fields. Paths are redacted (`%USERPROFILE%`, no machine name, no SIDs). The advisor is opt-in, off by default, with a dry-run preview of the exact payload. File and folder names are untrusted text (prompt-injection risk): treat as data, validate model output against a strict schema, and never let model output trigger an action.
7. **Never follow links.** Do not traverse junctions, symlinks, or other reparse points. Detect, record, and report them.
8. **Tests never touch real user data.** Use `tempfile` or `fixtures/`. The CLI refuses to scan a drive root unless `--i-know` is passed.
9. **Secrets:** API keys come from env vars or Windows Credential Manager only. Never in config files, logs, snapshots, or test output.
10. **Elevation:** the main app never runs as admin. The MFT helper is a separate binary, elevated on explicit user request, with a read-only protocol and a pipe ACL limited to the current user. No write-capable command may exist in its protocol.
11. **Fail closed.** On any error or ambiguity (parse failure, invalid model output, unresolvable path, changed file), the result is the more cautious one.
12. **Do not weaken or delete tests** to make them pass. Do not mark safety tests `#[ignore]`. If a safety test fails, fix the code.

## When to stop and ask the user
- Any task needs a destructive API, new elevated capability, network access outside `dc-advisor`, or a new `unsafe` block outside `dc-mft`/`dc-winmeta`.
- A rule you are asked to write would mark user-created data (documents, saves, profiles, photos, source code, VM disks, databases) as anything other than `DontTouch`.
- You are unsure whether something is safe to delete. When unsure, it is not safe.

## Definition of done for every task
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass; docs/CHANGELOG updated; end with a brief summary listing: what changed, what you did NOT do, and any safety-relevant decisions for human review.

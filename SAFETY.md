# Safety

DiskCleaner inspects entire disks and advises on deletion, so safety rules come before features.

1. **Read-only until the quarantine phase (B9).** No code path deletes, moves, renames or writes
   to scanned paths. The quarantine crate is behind the `destructive-actions` feature, off by default.
   Since B9 the removal engine exists there: it only *moves* items into a holding folder on the
   same drive, re-checks each one first, keeps an append-only journal, and deletes only when a
   cleanup is emptied, only inside a holding folder. No normal build includes it until the
   owner's test on a throwaway folder (see `docs/phases/b9-cleaning/design.md`).
2. **No destructive APIs outside `dc-quarantine`.** A Clippy `disallowed-methods` rule fails the
   build if `remove_file`, `remove_dir`, `remove_dir_all` or `rename` appear.
3. **Default-deny.** Anything unclassified is `DontTouch`.
4. **Risk can only go up.** `Safe < ProbablySafe < DontTouch`; the final risk is the maximum of
   the rule's and the advisor's. The AI advisor can only make a result more cautious.
5. **Protected Windows paths are compiled in** and cannot be overridden by rule packs.
6. **Metadata only to AI**, opt-in, off by default, with a preview of the exact payload.
7. **Links are never followed** (junctions, symlinks, reparse points).
8. **Tests never touch real user data**; they use temporary directories or generated fixtures.
9. **Fail closed.** Any error or ambiguity produces the more cautious result.

## Reporting a problem
Please open a private security advisory on the repository rather than a public issue for anything
that could cause data loss.

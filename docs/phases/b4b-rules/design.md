# Phase B4b: Rules and classification - Design

Status: **draft for review**. Requirements: `requirements.md`.

## 1. Crate layout (`dc-rules`, safe Rust)
```
src/
  lib.rs        API and the "classification is advice, not an action" note
  pack.rs       RulePack, Rule (serde, deny_unknown_fields), parse + validate
  validate.rs   the checks of R2, returning all problems at once
  builtin.rs    include_str!("../../../rules/junk/builtin.toml") and a cached parsed pack
  facts.rs      NodeFacts { kind, size, mtime, flags } (plain data)
  classify.rs   Classifier { protected, pack }: classify(path, facts, resolver) -> Classification
  folder.rs     classify a folder item from its descendants in the scan tree
  candidates.rs topmost-candidate selection and totals
  report.rs     text tables for the CLI
rules/junk/builtin.toml   the built-in pack (data, reviewable in one file)
```
Dependencies: `dc-core` (Risk, Tree), `dc-protected`, `serde`, `toml` (pinned). `dc-attrib` is **not** a dependency; attribution is joined only in the CLI for display.

## 2. Rule matching
Patterns reuse `dc_protected::Pattern` (made public with a stable API): normalized lower-case component patterns. `scope = subtree` matches the folder and all below; `self` only the item. The path is normalized with `dc_protected::normalize`; an invalid path is `DontTouch` (`invalid_path`). A rule matches if either the typed path or the resolver's resolved path matches (same escalate-only idea as B4a).

## 3. Classification algorithm
```
fn classify(path, facts, resolver):
  contributions = []
  protected = protected_list.check(path, resolver)
  if protected.protected: return DontTouch(reasons = protected.reasons)      // rules never consulted
  if facts.reparse or facts.cloud or facts.hardlink: contributions += DontTouch(reason)
  for rule in pack.rules where rule.matches(path, facts): contributions += rule.risk (+ reason)
  if no rule matched: contributions += DontTouch("unclassified")
  risk = Risk::escalate_all(risks)           // order independent
  Classification { risk, reasons (deduped, stable order), rules }
```
`min_age_days` is part of `matches`: a file younger than the limit simply does not match (so the result is unclassified, i.e. `DontTouch`). A folder item has no age of its own; its files are judged individually when listing items (see 4).

## 4. Folder items and candidates
A rule with `scope = subtree` on a folder (for example a cache folder) produces **one item for the folder**. To classify an item the engine looks at its tree:
1. The folder's own classification (path-based).
2. For each descendant node (walk limited to the scan tree): if it is protected (typed path), a link, cloud, hardlinked, or named `.git`, the item becomes `DontTouch` with reason `contains_protected_or_special` and the first offending path.
3. If the rule has `min_age_days`, the item is a candidate only if **every file** inside is older than that; otherwise it is reported with the age blocker (not a candidate).
Candidates are the topmost items with `Safe`/`ProbablySafe` (no ancestor already a candidate). Sizes come straight from the folder's aggregate, so no double counting.
Performance: descendant walks are only done for items that matched a rule (few hundred), not for the whole tree.

## 5. Validation (`validate.rs`)
A function `validate(pack) -> Vec<Problem>`; the loader refuses a pack with any problem. Checks: schema, unique ids, risk, non-empty text, pattern parse, minimum literal components (3), `Safe` rule has a naming literal and is not entirely inside protected, age range, examples present and consistent (matches/does not match; examples are run through the real `Classifier` with a fake resolver and must be `NotProtected` for `examples_match`). Duplicate or contradicting rules are allowed (the maximum wins) but a warning list is produced.

## 6. The D54 change (upgrade leftovers)
`dc-protected` currently protects every root name starting with `$`. B4b needs `$WINDOWS.~BT` / `$WINDOWS.~WS` to be classifiable as `ProbablySafe`. Change in `dc-protected` (reviewed, with its tests updated): the `$*` rule keeps protecting NTFS metadata names (`$MFT`, `$Boot`, `$Recycle.Bin`, ...) through an explicit list, and no longer matches `$WINDOWS.~BT` / `$WINDOWS.~WS`. They then fall under `Windows`-adjacent rules in the pack as `ProbablySafe`. A pinned test asserts every other `$`-name stays protected. The protected list still cannot be overridden by any pack.

## 7. CLI
`dc classify` loads the scan (and optionally the apps snapshot for owner names), builds `Classifier::builtin()` with the Windows known-folder extras as in `dc protected`, and prints: totals per risk, then candidates (size, risk, rule, path, owner). `--explain PATH` prints each step. `dc rules --list|--check`.

## 8. Risks
| Risk | Mitigation |
|---|---|
| A rule marks something valuable as Safe | Conservative first pack; required examples; never-Safe table; explicit DontTouch rules; final QA review of the pack; Safe still never deletes anything |
| Browser rules touch logins | Cache sub-folders only, exact folder names, explicit DontTouch rules for profile data |
| App running while its cache is "Safe" | Documented; quarantine phase (B9) will check for open files |
| Pattern too broad | Minimum three literal components, naming literal for Safe, validation at load |
| Different spellings bypass rules | Same normalization and resolver pairing as B4a, evasion tests |
| toml crate behaviour | Pinned version, `deny_unknown_fields`, tests on malformed packs |

## 9. Steps
1. Make `Pattern` public in `dc-protected`; D54 change + tests. 2. `pack`, `validate`, malformed-pack tests. 3. `classify` + never-Safe table + property tests. 4. `folder`, `candidates`. 5. Built-in pack with examples. 6. CLI `classify` and `rules`. 7. Docs, CHANGELOG, DECISIONS, NOTES; real-machine run and review of every Safe candidate.

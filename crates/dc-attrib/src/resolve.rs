//! Turn claims into one assignment per folder: strongest wins, near-ties are ambiguous.

use std::collections::BTreeMap;

use crate::model::{Assignment, Confidence, Evidence, OwnerId};
use crate::rules::Claim;

/// Claims closer than this many points make a folder ambiguous.
pub const AMBIGUITY_MARGIN: u8 = 10;

fn confidence_for(strength: u8) -> Confidence {
    match strength {
        80..=u8::MAX => Confidence::High,
        40..=79 => Confidence::Medium,
        _ => Confidence::Low,
    }
}

/// One assignment per node that has at least one claim, sorted by node id.
pub fn resolve(claims: &[Claim]) -> Vec<Assignment> {
    let mut by_node: BTreeMap<u32, Vec<&Claim>> = BTreeMap::new();
    for claim in claims {
        by_node.entry(claim.node).or_default().push(claim);
    }
    let mut out = Vec::with_capacity(by_node.len());
    for (node, list) in by_node {
        // Best claim per owner.
        let mut best: BTreeMap<OwnerId, Vec<&Claim>> = BTreeMap::new();
        for claim in list {
            best.entry(claim.owner).or_default().push(claim);
        }
        let mut owners: Vec<(OwnerId, u8, Vec<Evidence>)> = best
            .into_iter()
            .map(|(owner, claims)| {
                let strength = claims.iter().map(|c| c.strength).max().unwrap_or(0);
                let mut evidence: Vec<Evidence> =
                    claims.iter().map(|c| c.evidence.clone()).collect();
                evidence.sort_by(|a, b| (a.kind, &a.detail).cmp(&(b.kind, &b.detail)));
                evidence.dedup();
                (owner, strength, evidence)
            })
            .collect();
        // Strongest first; equal strength ordered by id for determinism.
        owners.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let Some((top_owner, top_strength, top_evidence)) = owners.first().cloned() else {
            continue;
        };
        let near: Vec<&(OwnerId, u8, Vec<Evidence>)> = owners
            .iter()
            .skip(1)
            .filter(|(_, s, _)| top_strength.saturating_sub(*s) < AMBIGUITY_MARGIN)
            .collect();
        let alternatives: Vec<OwnerId> = owners.iter().skip(1).map(|(o, _, _)| *o).collect();
        if near.is_empty() {
            out.push(Assignment {
                node,
                owner: Some(top_owner),
                confidence: confidence_for(top_strength),
                evidence: top_evidence,
                alternatives,
            });
        } else {
            let mut evidence = top_evidence;
            for (_, _, e) in &near {
                evidence.extend(e.iter().cloned());
            }
            out.push(Assignment {
                node,
                owner: None,
                confidence: Confidence::Ambiguous,
                evidence,
                alternatives: std::iter::once(top_owner).chain(alternatives).collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EvidenceKind;

    fn claim(node: u32, owner: u32, strength: u8) -> Claim {
        Claim {
            node,
            owner,
            strength,
            evidence: Evidence {
                kind: EvidenceKind::NameMatch,
                detail: format!("o{owner}"),
            },
        }
    }

    #[test]
    fn strongest_claim_wins_and_others_are_alternatives() {
        let result = resolve(&[claim(1, 5, 90), claim(1, 6, 40)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, Some(5));
        assert_eq!(result[0].confidence, Confidence::High);
        assert_eq!(result[0].alternatives, vec![6]);
    }

    #[test]
    fn near_ties_are_ambiguous_with_no_owner() {
        let result = resolve(&[claim(1, 5, 40), claim(1, 6, 40)]);
        assert_eq!(result[0].owner, None);
        assert_eq!(result[0].confidence, Confidence::Ambiguous);
        assert_eq!(result[0].alternatives, vec![5, 6]);
        // Exactly ten points apart is a clear win.
        assert_eq!(
            resolve(&[claim(1, 5, 40), claim(1, 6, 30)])[0].owner,
            Some(5)
        );
        assert_eq!(resolve(&[claim(1, 5, 40), claim(1, 6, 31)])[0].owner, None);
    }

    #[test]
    fn input_order_does_not_matter() {
        let a = resolve(&[claim(1, 5, 40), claim(2, 6, 90), claim(1, 7, 40)]);
        let b = resolve(&[claim(1, 7, 40), claim(2, 6, 90), claim(1, 5, 40)]);
        assert_eq!(a, b);
    }
}

//! Property tests for `Risk::escalate` and `Risk::escalate_all`.

use dc_core::Risk;
use proptest::prelude::*;

fn risk() -> impl Strategy<Value = Risk> {
    proptest::sample::select(Risk::ALL.to_vec())
}

proptest! {
    #[test]
    fn escalate_is_commutative(a in risk(), b in risk()) {
        prop_assert_eq!(a.escalate(b), b.escalate(a));
    }

    #[test]
    fn escalate_is_associative(a in risk(), b in risk(), c in risk()) {
        prop_assert_eq!(a.escalate(b).escalate(c), a.escalate(b.escalate(c)));
    }

    #[test]
    fn escalate_is_idempotent(a in risk()) {
        prop_assert_eq!(a.escalate(a), a);
    }

    #[test]
    fn safe_is_the_identity(a in risk()) {
        prop_assert_eq!(a.escalate(Risk::Safe), a);
        prop_assert_eq!(Risk::Safe.escalate(a), a);
    }

    #[test]
    fn result_is_never_less_cautious_than_either_input(a in risk(), b in risk()) {
        let r = a.escalate(b);
        prop_assert!(r >= a);
        prop_assert!(r >= b);
    }

    #[test]
    fn escalate_equals_max(a in risk(), b in risk()) {
        prop_assert_eq!(a.escalate(b), a.max(b));
    }

    #[test]
    fn any_collection_containing_dont_touch_yields_dont_touch(
        mut v in proptest::collection::vec(risk(), 0..20),
        at in any::<proptest::sample::Index>(),
    ) {
        let position = at.index(v.len() + 1);
        v.insert(position, Risk::DontTouch);
        prop_assert_eq!(Risk::escalate_all(v), Risk::DontTouch);
    }

    #[test]
    fn escalate_all_matches_the_maximum(v in proptest::collection::vec(risk(), 1..20)) {
        let expected = v.iter().copied().max();
        prop_assert_eq!(Some(Risk::escalate_all(v)), expected);
    }
}

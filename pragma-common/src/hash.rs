use std::cmp::Ordering;

use starknet::core::types::FieldElement;

/// The first element A of a pedersen hash (A,B) follows the rule:
/// A <= B
pub fn pedersen_hash(a: &FieldElement, b: &FieldElement) -> FieldElement {
    let (a_sorted, b_sorted) = match a.cmp(b) {
        Ordering::Less | Ordering::Equal => (a, b),
        Ordering::Greater => (b, a),
    };
    starknet::core::crypto::pedersen_hash(a_sorted, b_sorted)
}

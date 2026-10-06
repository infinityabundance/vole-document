//! Three-universe accounting for a cohort of store-backed roots (Phase 9).
//!
//! The three universes are kept **permanently distinct** (ADR-0020); conflating
//! them is what makes a content-addressed store look like a compressor.
//!
//! Notation: cohort `C = {r_0 .. r_{n-1}}`; `d_i` is the standalone descriptor
//! for root `i`, `e_i` its store-backed form; `reach(i)` is the set of external
//! object ids in root `i`'s object table; `O = ⋃_i reach(i)` is the unique
//! reachable set and `refcount(o) = |{ i : o ∈ reach(i) }|`.
//!
//! ```text
//! S (standalone)        = Σ_i |serialize(d_i)|
//! U (unique reachable)  = Σ_i |serialize(e_i)| + Σ_{o ∈ O} len(o)
//! A (amortized cohort)  = Σ_i ( |serialize(e_i)| + Σ_{o ∈ reach(i)} len(o)/refcount(o) )
//! ```
//!
//! [`account`] reports all three plus the per-root amortized split. Since
//! `Σ_i Σ_{o ∈ reach(i)} len(o)/refcount(o) = Σ_{o ∈ O} len(o)` telescopes,
//! `A == U` **exactly** by construction: the amortized total is not a fourth
//! number. See ADR-0020 for why the split rule is *fractional by reference
//! count*, and why it reduces to standalone with no sharing.
//!
//! Note that `S` is a whole-file number and is the only one comparable to a
//! per-file compressor; `U` and `A` include the shared object once and are **not**
//! comparable to a whole file (a store root alone is never a whole document).

use std::collections::BTreeMap;

use crate::container::{Descriptor, ObjectSource};
use crate::error::{Error, Result};
use crate::store::{Id, ObjectResolver, ObjectStore, hydrate};

/// Per-root accounting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootAccount {
    /// Serialized store-backed descriptor bytes `|serialize(e_i)|`.
    pub root_bytes: u64,
    /// Serialized standalone descriptor bytes `|serialize(d_i)|` (all objects
    /// inline).
    pub standalone_bytes: u64,
    /// Distinct external object ids reachable from this root.
    pub reachable_objects: u64,
    /// Integerized amortized bytes `A_i` (see [`account`] for the split rule).
    pub amortized_bytes: u64,
}

/// Cohort accounting across the three frozen universes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountReport {
    /// Per-root figures, in cohort order.
    pub roots: Vec<RootAccount>,
    /// `S`: the standalone universe (whole-file, comparable to per-file LZ).
    pub standalone_bytes: u64,
    /// `U`: the unique-reachable universe (roots + each unique object once).
    pub unique_reachable_bytes: u64,
    /// `A`: the amortized cohort universe. Equals `unique_reachable_bytes`.
    pub amortized_bytes: u64,
    /// `|O|`: number of unique reachable objects.
    pub unique_objects: u64,
    /// `Σ_{o ∈ O} len(o)`: raw bytes of the unique reachable objects.
    pub unique_object_bytes: u64,
    /// Reachable ids the store does not contain. A valid closure has none; a
    /// non-empty list is a live [`crate::ErrorClass::MissingExternalObject`] and
    /// [`account`] fails closed rather than reporting a universe it cannot prove.
    pub dangling: Vec<Id>,
}

/// Compute the three accounting universes for `roots`, resolving objects and
/// closure through `store`.
///
/// The amortized distribution rule is **fractional by reference count**: each
/// object `o` is split equally among the `refcount(o)` roots that reference it,
/// `q = len(o) / refcount(o)` to each, with the `len(o) % refcount(o)` remainder
/// bytes assigned one each to the lowest-index referencing roots. Every byte of
/// `len(o)` is thus distributed, so `Σ_i A_i == U` holds exactly with no rounding
/// drift. The rule is order-invariant in the sense that it depends only on
/// `reach(i)` and per-object `refcount`, and it reduces to standalone when every
/// `refcount(o) == 1`.
///
/// Fails closed with [`ErrorClass::MissingExternalObject`][crate::ErrorClass]
/// if any reachable object is absent.
pub fn account<R: ObjectResolver + ObjectStore>(
    roots: &[Descriptor],
    store: &R,
) -> Result<AccountReport> {
    // id -> (declared raw length, referencing root indices in ascending order).
    let mut objects: BTreeMap<Id, (u64, Vec<usize>)> = BTreeMap::new();
    for (i, root) in roots.iter().enumerate() {
        for src in &root.objects {
            if let ObjectSource::External { id, len } = src {
                let entry = objects.entry(*id).or_insert((*len, Vec::new()));
                if !entry.1.contains(&i) {
                    entry.1.push(i);
                }
            }
        }
    }

    // Closure: every reachable id must be present in the store.
    let mut dangling: Vec<Id> = Vec::new();
    for id in objects.keys() {
        if !store.contains(id)? {
            dangling.push(*id);
        }
    }
    if !dangling.is_empty() {
        return Err(Error::missing_external_object(format!(
            "{} reachable object(s) are not present in the store; the closure is not valid",
            dangling.len()
        )));
    }

    // Root/store-backed bytes and standalone bytes.
    let mut roots_out: Vec<RootAccount> = Vec::with_capacity(roots.len());
    let mut standalone_bytes: u64 = 0;
    let mut root_total: u64 = 0;
    for root in roots {
        let root_bytes = root.serialize()?.0.len() as u64;
        root_total += root_bytes;

        let mut standalone = root.clone();
        hydrate(&mut standalone, store)?;
        let standalone_len = standalone.serialize()?.0.len() as u64;
        standalone_bytes += standalone_len;

        roots_out.push(RootAccount {
            root_bytes,
            standalone_bytes: standalone_len,
            reachable_objects: 0,
            // The root bytes are always paid by this root; object shares are added
            // below.
            amortized_bytes: root_bytes,
        });
    }

    // Distribute each object's raw length across its referencing roots.
    let mut unique_object_bytes: u64 = 0;
    for (len, refs) in objects.values() {
        unique_object_bytes += *len;
        let k = refs.len() as u64;
        if k == 0 {
            continue;
        }
        let q = len / k;
        let r = len % k;
        for (j, &root_index) in refs.iter().enumerate() {
            let mut add = q;
            if (j as u64) < r {
                add += 1;
            }
            roots_out[root_index].amortized_bytes += add;
            roots_out[root_index].reachable_objects += 1;
        }
    }

    let unique_reachable_bytes = root_total + unique_object_bytes;
    let amortized_bytes: u64 = roots_out.iter().map(|r| r.amortized_bytes).sum();
    debug_assert_eq!(
        amortized_bytes, unique_reachable_bytes,
        "Σ amortized must equal unique reachable (the split telescopes)"
    );

    Ok(AccountReport {
        roots: roots_out,
        standalone_bytes,
        unique_reachable_bytes,
        amortized_bytes,
        unique_objects: objects.len() as u64,
        unique_object_bytes,
        dangling,
    })
}

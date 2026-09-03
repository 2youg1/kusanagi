// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What `Grant::from_canonical_bytes` does with bytes an inviter chose.
//!
//! A grant chain arrives inside an invitation, which is the one thing in this
//! program a stranger hands to a person to paste. Its first byte is a **count**,
//! and the steps behind it are fixed width, so one byte says how much to
//! allocate. That is the shape this file exists to pin: the count is compared
//! against the limit before anything is reserved for it, and a count larger than
//! the bytes that follow is a refusal rather than a partial chain.
//!
//! `attenuation.rs` already covers what a *valid* chain means. This covers what
//! an invalid one costs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::let_underscore_must_use,
    reason = "test code"
)]

use kusanagi_grant::Grant;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Anything at all decodes to an answer, never to a crash.
    #[test]
    fn any_bytes_at_all_produce_an_answer(bytes in prop::collection::vec(any::<u8>(), 0..800)) {
        let _ = Grant::from_canonical_bytes(&bytes);
    }

    /// A count is compared against the limit and against what arrived.
    ///
    /// One byte claiming two hundred steps is one byte to send; the steps behind
    /// it are thousands of bytes each, and none of them are here.
    #[test]
    fn a_count_with_nothing_behind_it_is_refused(count in 1_u8..=u8::MAX) {
        let error = Grant::from_canonical_bytes(&[count])
            .expect_err("a chain of steps that were never sent was accepted");
        prop_assert!(
            matches!(error.code(), "grant.too_long" | "grant.truncated"),
            "a count of {count} was answered with {}",
            error.code()
        );
    }
}

#[test]
fn an_empty_chain_is_not_a_chain() {
    assert!(Grant::from_canonical_bytes(&[]).is_err());
    assert!(Grant::from_canonical_bytes(&[0]).is_err());
}

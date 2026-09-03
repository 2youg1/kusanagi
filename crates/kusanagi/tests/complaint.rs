// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a caller is told when a verb refuses, asserted from outside.
//!
//! These live apart from `complaint.rs` because they are about the door rather
//! than about the enum: every one of them enters through the public interface a
//! script matches on, so nothing here can be kept green by a change that a
//! caller would notice.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

use kusanagi::Complaint;
use kusanagi_grant::GrantError;
use kusanagi_site::SiteError;

/// The codes a caller matches on are published, and the layer that produces
/// the failure does not know them. This is the only place the two meet, so
/// it is the only place the meeting can be checked.
#[test]
fn every_local_failure_arrives_with_the_code_it_had_before_the_split() {
    let cases = [
        (
            SiteError::Local {
                action: "read this endpoint's identity",
                source: std::io::Error::other("disk"),
            },
            "kusanagi.local",
        ),
        (
            SiteError::BadName {
                name: "with/slash".to_owned(),
                reason: "has a slash in it".to_owned(),
            },
            "kusanagi.malformed",
        ),
        (
            SiteError::BadInvitation {
                reason: "has no prefix".to_owned(),
            },
            "kusanagi.malformed",
        ),
        (
            SiteError::BadRecord {
                what: "a channel",
                reason: "is from another version".to_owned(),
            },
            "kusanagi.malformed",
        ),
        (
            SiteError::UnknownChannel {
                name: "nobody".to_owned(),
            },
            "kusanagi.unknown_channel",
        ),
        (SiteError::Grant(GrantError::Empty), "grant.empty"),
    ];
    for (error, code) in cases {
        assert_eq!(Complaint::from(error).code(), code);
    }
}

/// A failure with no way forward is a failure a caller cannot act on.
#[test]
fn a_local_failure_still_carries_a_way_out() {
    let complaint = Complaint::from(SiteError::UnknownChannel {
        name: "nobody".to_owned(),
    });
    assert!(complaint.render(false).contains("kusanagi channels"));
}

/// Advice about an invitation is for somebody who has one. Anything else
/// sends a confused caller to look for a thing they never had — which is
/// what the three kinds of malformed exist to prevent.
#[test]
fn a_mistyped_name_is_not_told_to_copy_an_invitation() {
    let complaint = Complaint::from(SiteError::BadName {
        name: "Upper".to_owned(),
        reason: "has a capital in it".to_owned(),
    });
    let rendered = complaint.render(false);
    assert!(!rendered.contains("kusanagi2:"), "{rendered}");
    assert!(rendered.contains("a-z"), "{rendered}");
}

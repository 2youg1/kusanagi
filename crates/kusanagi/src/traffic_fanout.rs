// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a group hears: one segment per member, each on its own channel.
//!
//! Apart from `traffic.rs` because the two change for different reasons: one
//! segment out is the shape of a send, and N segments out is the shape of a
//! fan-out with per-member results. `appended` stays where it is; this calls
//! it once per member.

use kusanagi_door::{Complaint, Delivery, Landed, Outcome};
use kusanagi_kernel::{Freight, Instant};
use kusanagi_site::Site;

use crate::assembly::signer;
use crate::traffic::appended;

/// Appends one segment to every member of a group, and reports each separately.
///
/// **One member's failure is not the send's failure.** A host that is down, a
/// channel that was forgotten, or a grant that was revoked stops that member
/// from hearing this and stops nothing else; collapsing the five results into
/// one would either hide a person who did not receive it or claim four people
/// did not when they did. The caller reads the rows.
///
/// # Errors
///
/// [`Complaint::UnknownGroup`] when there is no such group. That is the one
/// failure of the fan-out itself rather than of a member.
pub(crate) fn fanout(
    site: &Site,
    group: &str,
    payload: &[u8],
    now: Instant,
) -> Result<Outcome, Complaint> {
    // One signer for every member: N members used to cost N identity reads.
    let me = signer(site)?;
    let roster = site.roster(group).map_err(|error| match error {
        kusanagi_site::SiteError::UnknownChannel { name } => Complaint::UnknownGroup { name },
        other => other.into(),
    })?;
    let delivered = roster
        .members
        .iter()
        .map(|member| Delivery {
            member: member.clone(),
            landed: match Freight::message(payload.to_vec())
                .map_err(Complaint::from)
                .and_then(|freight| appended(site, &me, member, freight, now))
            {
                Ok(written) => Landed::Sent {
                    index: written.index,
                    address: written.address,
                },
                Err(refusal) => Landed::Refused {
                    code: refusal.code(),
                    error: refusal.to_string(),
                },
            },
        })
        .collect();
    Ok(Outcome::FannedOut {
        group: group.to_owned(),
        delivered,
    })
}

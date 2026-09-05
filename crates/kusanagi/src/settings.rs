// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The three things a site records about itself: how it reaches a host, how
//! wide it reads one, and what it asks to be called.
//!
//! Both are settings rather than flags for one reason: a scheduler task and a
//! person at a terminal must behave the same way, or the endpoint that differs
//! between the two is the one the host can tell apart. Each is read back after
//! it is written, so the answer is what the disk says and never what was asked.

use kusanagi_door::{Complaint, Outcome};
use kusanagi_kernel::Alias;
use kusanagi_site::{Egress, Site};

use crate::assembly::identity;
use crate::request::Naming;

/// Reads or records how many hex digits of its ward this site names in a read.
pub(crate) fn crowd(site: &Site, digits: Option<u8>) -> Result<Outcome, Complaint> {
    if let Some(digits) = digits {
        site.set_sweep_digits(digits)?;
    }
    let digits = site.sweep_digits()?.unwrap_or(kusanagi_walk::DIGITS);
    Ok(Outcome::Sweeping {
        digits,
        wards: 16_u32.saturating_pow(u32::from(kusanagi_site::MOST_DIGITS.saturating_sub(digits))),
    })
}

/// Reads, records or clears what this endpoint asks to be called, and reports
/// the identity as it now stands.
///
/// The name is checked here, where the argument was typed, so an unfit one is
/// an argument error with what to pass instead — not a record refusal later.
pub(crate) fn named(site: &Site, naming: &Naming) -> Result<Outcome, Complaint> {
    match naming {
        Naming::Ask => {}
        Naming::Clear => site.set_alias(None)?,
        Naming::Set(text) => {
            let alias = Alias::new(text).map_err(|error| Complaint::Argument {
                what: "--as",
                reason: error.to_string(),
                instead: "pass one printable line of at most 32 bytes, with no control characters",
            })?;
            site.set_alias(Some(&alias))?;
        }
    }
    identity(site)
}

/// Reads or records whether this site may reach a host without a proxy.
pub(crate) fn egress(site: &Site, require: Option<bool>) -> Result<Outcome, Complaint> {
    if let Some(required) = require {
        site.set_egress(if required {
            Egress::ProxyRequired
        } else {
            Egress::Free
        })?;
    }
    Ok(Outcome::Egress {
        proxy_required: site.egress()? == Egress::ProxyRequired,
    })
}

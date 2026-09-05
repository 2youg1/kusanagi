// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A whole site in one sealed byte string, and back again.
//!
//! Apart from `assembly.rs` because that router is at its line limit: sealing
//! everything and restoring it are two verbs that touch the archive format and
//! nothing else on the network.

use kusanagi_site::Site;

use kusanagi_door::{Complaint, Outcome};

use crate::world::fresh_seed;
use zeroize::Zeroize as _;

/// Seals everything this endpoint holds under a key drawn here and shown once.
///
/// **The recovery key is generated rather than chosen.** A passphrase somebody
/// invents is a passphrase somebody guesses, and there is no rate limit on a
/// file. Thirty-two bytes from the operating system, in hexadecimal, handed back
/// exactly once: whoever runs this writes it down or loses the archive.
pub(crate) fn export(site: &Site) -> Result<Outcome, Complaint> {
    let mut recovery = fresh_seed()?;
    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(fresh_seed()?.get(..12).unwrap_or(&[0; 12]));
    let archive = kusanagi_site::export(site, &recovery, nonce)?;
    let key = kusanagi_kernel::Hex(&recovery).to_string();
    recovery.zeroize();
    Ok(Outcome::Exported {
        recovery: key,
        archive,
    })
}

/// Puts an archive back into a root that has nothing in it.
pub(crate) fn import(
    site: &Site,
    recovery: &[u8; 32],
    archive: &[u8],
) -> Result<Outcome, Complaint> {
    kusanagi_site::import(site, recovery, archive)?;
    let names = site.names()?;
    Ok(Outcome::Imported {
        site: site.root().display().to_string(),
        channels: names.len(),
    })
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a segment is left for its reader.
//!
//! This module declares the address and deliberately does **not** know how one is
//! produced. Derivation needs a shared secret, and a secret in the noun layer
//! would put the one fact that makes this network private into the crate with
//! the widest reach. The single authority for producing an address is
//! `kusanagi_seal::derive`.

use crate::identifier;

identifier! {
    /// An opaque address that receives exactly one segment.
    ///
    /// 160 bits: wide enough that two independently derived addresses do not
    /// collide, narrow enough that the text form is 40 characters, which is a
    /// comfortable object-storage key.
    ///
    /// Nothing about an address says who wrote to it, who reads it, or what came
    /// before it. Two addresses of the same channel are related only through a
    /// secret the host does not have.
    DropAddr, 20
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::DropAddr;
    use core::str::FromStr;

    #[test]
    fn survives_a_text_round_trip() {
        let addr = DropAddr::from_bytes([7_u8; 20]);
        assert_eq!(DropAddr::from_str(&addr.to_string()).unwrap(), addr);
        assert_eq!(addr.to_string().len(), 40);
    }
}

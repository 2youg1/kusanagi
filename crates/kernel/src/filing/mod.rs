// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How a host files a drop, and how a reader asks for a set of them.
//!
//! A reader used to name the address it wanted, which handed the host the one
//! relation this network exists to hide: the writer of an address and its reader,
//! paired on the host's own access log. `ARCHITECTURE.md` §9 D-20 rules that a
//! read names a **bin** instead — a public period and a ward — and takes every
//! object in it, so what a reader asks for is a function of public data and of
//! nothing it knows.
//!
//! Three nouns hold that:
//!
//! - a [`Ward`] is the sixteen-bit number a reader picks once, when its identity
//!   is made, and hands to a writer inside an invitation;
//! - a [`Period`] is which stretch of public time a drop was written in, so that
//!   a bin is finite and a host can expire one whole;
//! - an [`Object`] is those two and an address: **the whole of what a host is
//!   told**, and the only thing a [`Waypoint`](crate::Waypoint) method takes.
//!
//! One type rather than a bin parameter beside every address is what keeps the
//! seam honest: an adapter cannot file a drop in one bin and read it from
//! another, and a listing hands back exactly what a read takes.
//!
//! A [`Sweep`] names a set of bins by the leading hex digits of their ward.
//! Fewer digits is a larger anonymity set at the cost of bandwidth, and it is a
//! knob the reader turns **alone**: no writer, no host and no peer has to agree.

use core::fmt;
use core::str::FromStr;

use crate::address::DropAddr;

mod sweep;

pub use sweep::Sweep;

/// Which reader's corner of a host a drop is filed in.
///
/// Sixteen bits, chosen at random when an identity is made and never derived
/// from anything: a ward that could be computed from a handle would let a host
/// work out whose corner it is looking at, which is the whole of what this hides.
/// It is not a secret — every writer to this endpoint is told it, and the host
/// reads it off every key — it is a **crowd**, and its worth is the number of
/// readers who share it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ward(u16);

impl Ward {
    /// How many hex digits a ward is written in.
    pub const DIGITS: u8 = 4;

    /// The ward this number names.
    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// The number, for a caller that has to write it down.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Ward {
    /// Four lower-case hex digits, always, because it is a key prefix.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{:04x}", self.0)
    }
}

/// Which stretch of public time a drop was written in.
///
/// Public on purpose. A host has always seen when a drop arrived, so putting the
/// period in the key gives away nothing it did not have, and it buys two things:
/// a bin small enough to take whole, and one lifetime for every object in it, so
/// that a host expiring a bin cannot tell a channel that releases from one that
/// keeps.
///
/// The number is a count of periods since the epoch. `kusanagi_seal::period` is
/// the one place a clock becomes one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Period(u64);

impl Period {
    /// The period this count names.
    #[must_use]
    pub const fn from_count(count: u64) -> Self {
        Self(count)
    }

    /// The count, for a caller stepping through a range of them.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Period {
    /// Sixteen lower-case hex digits, so that keys sort in time order.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{:016x}", self.0)
    }
}

/// One period of one ward: everything a reader takes in a single request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bin {
    period: Period,
    ward: Ward,
}

impl Bin {
    /// The bin a drop written in `period` for the reader of `ward` belongs to.
    #[must_use]
    pub const fn new(period: Period, ward: Ward) -> Self {
        Self { period, ward }
    }

    /// Which stretch of time.
    #[must_use]
    pub const fn period(self) -> Period {
        self.period
    }

    /// Whose corner.
    #[must_use]
    pub const fn ward(self) -> Ward {
        self.ward
    }
}

impl fmt::Display for Bin {
    /// `period/ward`, which is every key's leading two components.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}/{}", self.period, self.ward)
    }
}

/// The whole of what a host is told about one drop.
///
/// Every [`Waypoint`](crate::Waypoint) method takes one of these and nothing
/// else, so there is no way to write into one bin and read from another, and no
/// adapter has to remember to join two arguments into a key the same way twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Object {
    bin: Bin,
    addr: DropAddr,
}

impl Object {
    /// The drop at `addr`, filed in `bin`.
    #[must_use]
    pub const fn new(bin: Bin, addr: DropAddr) -> Self {
        Self { bin, addr }
    }

    /// Which bin it is filed in.
    #[must_use]
    pub const fn bin(&self) -> Bin {
        self.bin
    }

    /// Which address it is, which is the only part derived from a secret.
    #[must_use]
    pub const fn addr(&self) -> DropAddr {
        self.addr
    }
}

impl fmt::Display for Object {
    /// `period/ward/address`: the key on every adapter, spelled once.
    ///
    /// Here rather than in each adapter because a key spelled two ways is two
    /// key spaces, and the second one is discovered by a reader that finds
    /// nothing where a writer left something.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}/{}", self.bin, self.addr)
    }
}

/// Why a key a host handed back is not one of these.
///
/// One shape and no detail, because there is nothing a caller does differently
/// for a key with two components than for one whose ward is not hex: a key that
/// does not parse is a key this reader did not write, and it is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAKey;

impl fmt::Display for NotAKey {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("not `period/ward/address`")
    }
}

impl core::error::Error for NotAKey {}

impl FromStr for Object {
    type Err = NotAKey;

    /// Reads back exactly what [`Object`]'s [`fmt::Display`] wrote.
    ///
    /// Beside it so that the one spelling of a key has one authority: a listing
    /// is parsed by whoever asked for it, and a parser living in an adapter
    /// would be a second spelling discovered on the day the two disagree.
    fn from_str(text: &str) -> Result<Self, NotAKey> {
        let mut parts = text.split('/');
        let (Some(period), Some(ward), Some(addr), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(NotAKey);
        };
        if period.len() != 16 || ward.len() != usize::from(Ward::DIGITS) {
            return Err(NotAKey);
        }
        Ok(Self::new(
            Bin::new(
                Period::from_count(u64::from_str_radix(period, 16).map_err(|_| NotAKey)?),
                Ward::from_bits(u16::from_str_radix(ward, 16).map_err(|_| NotAKey)?),
            ),
            DropAddr::from_str(addr).map_err(|_| NotAKey)?,
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{Bin, Object, Period, Ward};
    use crate::address::DropAddr;

    fn object(period: u64, ward: u16, byte: u8) -> Object {
        Object::new(
            Bin::new(Period::from_count(period), Ward::from_bits(ward)),
            DropAddr::from_bytes([byte; 20]),
        )
    }

    #[test]
    fn a_key_is_the_period_the_ward_and_the_address() {
        let at = object(1, 0x00ab, 0x7f);
        assert_eq!(
            at.to_string(),
            format!("0000000000000001/00ab/{}", at.addr())
        );
    }

    #[test]
    fn a_key_reads_back_as_the_object_that_wrote_it() {
        let at = object(0x2b, 0x00ab, 0x7f);
        assert_eq!(at.to_string().parse::<Object>().unwrap(), at);
        for bad in [
            "0000000000000001/00ab",
            "0000000000000001/00ab/xx",
            "1/00ab/0000000000000000000000000000000000000000",
            "0000000000000001/0ab/0000000000000000000000000000000000000000",
            "0000000000000001/00ab/0000000000000000000000000000000000000000/more",
        ] {
            assert!(bad.parse::<Object>().is_err(), "{bad} parsed");
        }
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Which period one author's lane on one channel was last swept through, and
//! what the bin listed when it was.
//!
//! The cairn says how far a lane was *verified*; this says how far the host was
//! *asked*. They are two records because they move at different moments: a poll
//! that finds a new object in the bin moves this one whether or not it was for
//! this lane, and a poll that finds the bin as it was moves neither. Both are
//! recomputable, so the read side follows `cairns`' rule — every way of failing
//! to read one is reported as not having one, and the sweep starts again from
//! the period the channel was opened in. Losing every one of these costs bins,
//! never messages.
//!
//! **The listing is what lets a poll cost what arrived and no more.** A reader
//! that took a bin whole once needs only what the host lists *beyond* that, and
//! the decision is a function of two listings the host itself served — every
//! reader of the ward that polled at those two moments fetches the same set —
//! so the host learns nothing from the fetches it does not see. The keys kept
//! are the host's own public key names; none is derived from a secret this
//! endpoint holds, and none says which of them was wanted.
//!
//! ```text
//! period    8 bytes   big endian
//! count     2 bytes   big endian
//! keys      count × 62 bytes   `period/ward/address`, sorted
//! ```

use std::path::{Path, PathBuf};

use kusanagi_kernel::{Object, Period};

use crate::error::SiteError;
use kusanagi_vault as vault;

/// One lane's last sweep: the period, and every key its bin listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swept {
    /// The last period swept through.
    pub through: Period,
    /// Every key the bin listed then, sorted, so that the same set in any order
    /// is the same record.
    pub objects: Vec<Object>,
}

/// The exact width of one key as written.
const KEY_WIDTH: usize = 16 + 1 + 4 + 1 + 40;

impl Swept {
    /// What `period`'s bin listed.
    #[must_use]
    pub fn of(period: Period, listed: &[Object]) -> Self {
        let mut objects = listed.to_vec();
        objects.sort_unstable();
        objects.dedup();
        Self {
            through: period,
            objects,
        }
    }

    /// Whether `object` was listed the last time this bin was swept.
    #[must_use]
    pub fn lists(&self, object: &Object) -> bool {
        self.objects.binary_search(object).is_ok()
    }

    /// This listing once `object` has been written into the bin.
    ///
    /// **What the host would list, computed rather than asked for.** A writer
    /// that just added to a bin knows exactly how the listing changed, so the
    /// next sweep finds the bin as it was left and fetches nothing for the one
    /// drop this endpoint wrote itself. An object from another period is not
    /// this bin's and is left out.
    #[must_use]
    pub fn including(mut self, object: Object) -> Self {
        if object.bin().period() == self.through
            && let Err(at) = self.objects.binary_search(&object)
        {
            self.objects.insert(at, object);
        }
        self
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.through.count().to_be_bytes().to_vec();
        let count = u16::try_from(self.objects.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&count.to_be_bytes());
        for object in self.objects.iter().take(usize::from(count)) {
            out.extend_from_slice(object.to_string().as_bytes());
        }
        out
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (period, rest) = bytes.split_at_checked(8)?;
        let (count, mut rest) = rest.split_at_checked(2)?;
        let count = usize::from(u16::from_be_bytes(<[u8; 2]>::try_from(count).ok()?));
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            let (key, after) = rest.split_at_checked(KEY_WIDTH)?;
            objects.push(core::str::from_utf8(key).ok()?.parse().ok()?);
            rest = after;
        }
        if !rest.is_empty() {
            return None;
        }
        Some(Self::of(
            Period::from_count(u64::from_be_bytes(<[u8; 8]>::try_from(period).ok()?)),
            &objects,
        ))
    }
}

/// Where one channel's sweep records sit, under the same filed name as its
/// record and its cairns.
pub(crate) fn dir(root: &Path, filed: &str) -> PathBuf {
    root.join("sweeps").join(filed)
}

/// This lane's last sweep, if any record survives.
pub(crate) fn read(root: &Path, filed: &str, filed_author: &str) -> Option<Swept> {
    vault::read(&dir(root, filed).join(filed_author), "read a sweep record")
        .ok()
        .flatten()
        .and_then(|bytes| Swept::from_bytes(&bytes))
}

/// Writes down this lane's last sweep.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be written.
pub(crate) fn write(
    root: &Path,
    filed: &str,
    filed_author: &str,
    swept: &Swept,
) -> Result<(), SiteError> {
    let directory = dir(root, filed);
    vault::create_dir(&directory, "create the sweep directory")?;
    vault::write(
        &directory.join(filed_author),
        &swept.to_bytes(),
        "write a sweep record",
    )
    .map_err(Into::into)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::Swept;
    use kusanagi_kernel::{Bin, DropAddr, Object, Period, Ward};

    fn object(byte: u8) -> Object {
        Object::new(
            Bin::new(Period::from_count(7), Ward::from_bits(1)),
            DropAddr::from_bytes([byte; 20]),
        )
    }

    #[test]
    fn the_same_keys_in_another_order_are_the_same_record_and_read_back() {
        let one = Swept::of(Period::from_count(7), &[object(1), object(2)]);
        let two = Swept::of(Period::from_count(7), &[object(2), object(1)]);
        let three = Swept::of(Period::from_count(7), &[object(2)]);
        assert_eq!(one, two);
        assert_ne!(one, three);
        assert!(one.lists(&object(2)) && !three.lists(&object(1)));
        assert_eq!(Swept::from_bytes(&one.to_bytes()).unwrap(), one);
        assert_eq!(
            Swept::from_bytes(&Swept::of(Period::from_count(7), &[]).to_bytes())
                .unwrap()
                .objects
                .len(),
            0
        );
        let mut trailing = one.to_bytes();
        trailing.push(0);
        assert!(Swept::from_bytes(&trailing).is_none());
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How wide this site sweeps, and how full a bin it will still take.
//!
//! Four digits is one ward; every digit dropped multiplies by sixteen both the
//! wards a read is indistinguishable among and the bytes it downloads. The
//! choice is the reader's alone — no writer, host or peer is told — and it is a
//! site record rather than a flag so that a scheduler task and a person at a
//! terminal sweep the same width: a reader that widened once and then narrowed
//! would be the one reader in the ward whose requests changed shape.
//!
//! The cap is the other half of the same decision: how many objects one bin may
//! list before this reader gives up on that period rather than download it.
//! Both are one reader's willingness to pay, so they are one record.
//!
//! ```text
//! 4        the width alone, and the build's cap
//! 4 1024   the width and a cap this reader chose
//! ```
//!
//! Absent means the build's default for both.

use std::path::Path;

use crate::error::SiteError;
use crate::site::Site;
use kusanagi_vault as vault;

const FILE: &str = "sweep";

/// The most digits a ward has, and so the most this record may say.
pub const MOST_DIGITS: u8 = 4;

/// The fewest objects a reader may agree to take from one bin.
///
/// Below this a poll would refuse ordinary traffic rather than a flood, and a
/// cap that refuses ordinary traffic is a channel that has stopped working.
pub const LEAST_CAP: usize = 32;

/// The most objects a reader may agree to take from one bin.
///
/// Four thousand and ninety-six drops is half a gibibyte for one catch-up. It
/// is far past what anybody should want and it is still a number rather than no
/// number, because the bin is filled by strangers.
pub const MOST_CAP: usize = 4_096;

/// What this site has said about sweeping: the width, and the cap when it chose
/// one.
struct Chosen {
    digits: u8,
    cap: Option<usize>,
}

fn read(root: &Path) -> Result<Option<Chosen>, SiteError> {
    let Some(bytes) = vault::read(&root.join(FILE), "read the sweep record")? else {
        return Ok(None);
    };
    let bad = |reason: &str| SiteError::BadRecord {
        what: "the sweep record",
        reason: reason.to_owned(),
    };
    let text = core::str::from_utf8(bytes.trim_ascii())
        .map_err(|_| bad("this file does not hold text"))?;
    let mut said = text.split_ascii_whitespace();
    let digits = match said.next().map(str::as_bytes) {
        Some([digit @ b'0'..=b'4']) => digit.saturating_sub(b'0'),
        _ => return Err(bad("this file does not begin with one digit from 0 to 4")),
    };
    let cap = match said.next() {
        None => None,
        Some(count) => Some(
            count
                .parse::<usize>()
                .map_err(|_| bad("what follows the width is not a count of objects"))?,
        ),
    };
    if said.next().is_some() {
        return Err(bad("this file says more than a width and a cap"));
    }
    Ok(Some(Chosen { digits, cap }))
}

fn write(root: &Path, chosen: &Chosen) -> Result<(), SiteError> {
    vault::create_dir(root, "create the site directory")?;
    let said = match chosen.cap {
        None => format!("{}", chosen.digits.min(MOST_DIGITS)),
        Some(cap) => format!(
            "{} {}",
            chosen.digits.min(MOST_DIGITS),
            cap.clamp(LEAST_CAP, MOST_CAP)
        ),
    };
    vault::write(&root.join(FILE), said.as_bytes(), "write the sweep record").map_err(Into::into)
}

impl Site {
    /// How many hex digits of its ward this site names when it sweeps, when it
    /// has said.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be read, and
    /// [`SiteError::BadRecord`] when it holds something other than one digit.
    pub fn sweep_digits(&self) -> Result<Option<u8>, SiteError> {
        Ok(read(&self.root)?.map(|chosen| chosen.digits))
    }

    /// How many objects this site will take from one bin, when it has said.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be read, and
    /// [`SiteError::BadRecord`] when it does not hold a width and a cap.
    pub fn sweep_cap(&self) -> Result<Option<usize>, SiteError> {
        Ok(read(&self.root)?.and_then(|chosen| chosen.cap))
    }

    /// Records either half of the sweep record, leaving the other as it stands.
    ///
    /// Both are held by one file because they are one decision — what this
    /// reader will pay to stay indistinguishable — and a caller that set one
    /// would otherwise silently forget the other. `digits` saturates at
    /// [`MOST_DIGITS`] and `cap` is clamped to [`LEAST_CAP`]..=[`MOST_CAP`].
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be written, and
    /// [`SiteError::BadRecord`] when what is there now cannot be read.
    pub fn set_sweeping(&self, digits: Option<u8>, cap: Option<usize>) -> Result<(), SiteError> {
        let standing = read(&self.root)?;
        let chosen = Chosen {
            digits: digits
                .or_else(|| standing.as_ref().map(|chosen| chosen.digits))
                .unwrap_or(MOST_DIGITS),
            cap: cap.or_else(|| standing.and_then(|chosen| chosen.cap)),
        };
        write(&self.root, &chosen)
    }
}

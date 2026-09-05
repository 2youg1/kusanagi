// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Which of the segments a walk found are things somebody meant to say.
//!
//! One place decides it, so a channel read and a room read cannot disagree
//! about what a stream says. Three kinds of segment are on a stream and are not
//! messages: a filler, which exists so a silent endpoint looks like a busy one;
//! a roster, which a room replaces rather than shows; and a part, which is a
//! message only once the rest of its run is there.
//!
//! **Joining is a function of the segments alone.** Nothing is written down and
//! nothing is carried between reads: a run is either complete in what the walk
//! holds or it is not a message yet. That is what keeps a reader's memory to
//! positions — where it got to — and never to half of somebody else's file.

use std::borrow::Cow;

use kusanagi_kernel::{Part, Period, Purpose};

use crate::stepping::Held;

/// One thing an author meant to say, as a reader is to see it.
pub struct Message<'a> {
    /// The height it stands at, which for a divided message is its last part's.
    pub index: u64,
    /// How many of the reader's own segments its author had verified.
    pub acknowledged: u64,
    /// The period its author filed it in.
    pub filed: Period,
    /// The bytes, borrowed when one segment carried them and joined when a run
    /// did.
    pub payload: Cow<'a, [u8]>,
}

/// Every message on the stretch of stream a walk fetched, in order.
///
/// A run of parts becomes one message standing where its last part stands. A
/// run that is cut short — by a segment that is not its next part, or by the
/// end of what the walk holds — yields nothing at all and stops nothing after
/// it: a writer killed halfway and a writer still going are the same thing seen
/// from here, so neither is reported and neither is an error.
#[must_use]
pub fn messages(held: &[Held]) -> Vec<Message<'_>> {
    let mut said = Vec::with_capacity(held.len());
    let mut run: Vec<Part<'_>> = Vec::new();
    for one in held {
        let (index, acknowledged, filed) =
            (one.segment.index(), one.segment.acknowledged(), one.filed);
        match one.segment.purpose() {
            Purpose::Message => {
                run.clear();
                said.push(Message {
                    index,
                    acknowledged,
                    filed,
                    payload: Cow::Borrowed(one.segment.payload()),
                });
            }
            Purpose::Part => {
                let Some(part) = Part::of(one.segment.payload()) else {
                    run.clear();
                    continue;
                };
                let follows = usize::from(part.index) == run.len()
                    && run.first().is_some_and(|first| first.total == part.total);
                if part.index == 0 {
                    run.clear();
                } else if !follows {
                    run.clear();
                    continue;
                }
                run.push(part);
                if run.len() == usize::from(part.total) {
                    let mut joined =
                        Vec::with_capacity(run.iter().map(|part| part.bytes.len()).sum());
                    for part in run.drain(..) {
                        joined.extend_from_slice(part.bytes);
                    }
                    said.push(Message {
                        index,
                        acknowledged,
                        filed,
                        payload: Cow::Owned(joined),
                    });
                }
            }
            Purpose::Filler | Purpose::Roster => run.clear(),
        }
    }
    said
}

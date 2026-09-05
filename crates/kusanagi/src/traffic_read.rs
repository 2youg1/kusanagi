// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a channel read answers: a walk, turned into rows.
//!
//! Apart from `traffic.rs` because the two change for different reasons: that
//! file moves bytes between this machine and a host, and this one decides which
//! of the verified segments are things somebody meant to say. Joining a run of
//! parts back into its message lives here rather than at the call sites so that
//! `read` and `mine` cannot disagree about what a stream says.

use kusanagi_door::Outcome;
use kusanagi_kernel::Alias;
use kusanagi_walk::{Walked, messages};

/// Turns a walk into the answer for it, dropping what the caller already holds
/// and what nobody meant to say.
///
/// Two filters, and they are different kinds of thing. `--after` is a property
/// of the request, so it lives with the verb rather than in `door`, which
/// renders what it is handed and cannot perform a walk. **A filler is filtered
/// because it is not a message at all**: it exists so that an observer cannot
/// tell a silent endpoint from a busy one, and reporting it to the caller would
/// hand them padding to read as though somebody had written it.
///
/// The height is unaffected by either filter. It is the verified head of the
/// stream, fillers included — a height that skipped them would tell a reader
/// exactly how many slots went by empty, which is the fact the fillers were
/// spent to hide.
pub(crate) fn reported(
    name: &str,
    author: &str,
    alias: Option<&Alias>,
    walked: &Walked,
    after: Option<u64>,
) -> Outcome {
    let said = messages(walked.held());
    Outcome::read(
        name,
        author,
        alias.map(Alias::as_str),
        walked.head().map(|head| head.index()),
        said.iter()
            .filter(|message| after.is_none_or(|floor| message.index > floor))
            .map(|message| {
                (
                    message.index,
                    message.acknowledged,
                    message.payload.as_ref(),
                )
            }),
    )
}

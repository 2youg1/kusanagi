// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A room read, said to a person.
//!
//! Apart from `prose.rs` because it answers a different question: not one
//! stream, but one author's section per row. Each section is shaped like a
//! stream header, because that is what it is — one author's verified stream.

use crate::fence::Fence;
use crate::report::Outcome;
use crate::rows::{Thread, called};

/// Reports a room read: one verified stream per author.
///
/// Each row arrives as `(author, height, segments)`, with segments as
/// `(index, filed period, payload)` rather than as the walks they came from,
/// because a walk is a thing this crate must not be able to perform. Which of
/// them to show is the verb's decision and stays with the verb; how to render
/// them is this crate's and stays here.
#[must_use]
pub fn reported<'a>(
    name: &str,
    threads: impl IntoIterator<Item = (String, Option<u64>, Vec<(u64, u64, &'a [u8])>)>,
) -> Outcome {
    Outcome::Room {
        name: name.to_owned(),
        threads: threads
            .into_iter()
            .map(|(author, height, segments)| Thread::of(author, height, segments))
            .collect(),
    }
}

/// What a founded room says: its ward and its founder.
///
/// Apart from `render` because that dispatch is at its line limit. The four
/// room sentences live here beside the room read, each a function of its
/// fields rather than an arm that would push the dispatch past one hundred
/// lines.
pub(crate) fn founded(name: &str, ward: &str, founder: &str) -> String {
    format!(
        "room `{name}` is open\n  ward     {ward}\n  founder  {founder}\n\
         every member sweeps that ward, and every member's stream derives from one secret."
    )
}

/// What a room invitation says: the line, and the check code beside it.
pub(crate) fn invited(name: &str, invite: &str, check: &str, expires_at: u64) -> String {
    format!(
        "room `{name}` invitation, until {expires_at}\n\n{invite}\n\n\
         hand that line over once. Anybody who holds it can join, so treat it \
         the way you would treat a key.\n\n\
         check code {check} \u{2014} read it out to whoever you gave the line to."
    )
}

/// What an accepted room invitation says: who arrived, and who founded it.
pub(crate) fn joined(name: &str, handle: &str, founder: &str, check: &str) -> String {
    format!(
        "joined room `{name}`\n  you       {handle}\n  founder   {founder}\n\
         \n  check code {check} \u{2014} it should match what the person who invited you says"
    )
}

/// What a segment appended in a room says: where it was left.
pub(crate) fn sent(name: &str, index: u64, address: &str) -> String {
    format!("sent in room `{name}` #{index}\n  address {address}")
}

/// A room read: one author's section per row, in roster order.
///
/// An author who has written nothing gets one line rather than a section, so
/// silence reads as silence rather than as an empty conversation.
pub(crate) fn room(name: &str, threads: &[Thread], fence: Fence) -> String {
    let mut lines = vec![format!("`{name}`: {} author(s)", threads.len())];
    for thread in threads {
        let who = called(None, &thread.author);
        match thread.height {
            None => lines.push(format!("  {who} has written nothing yet")),
            Some(height) => {
                lines.push(format!(
                    "  {who} verifies to height {height} ({} segment(s))",
                    thread.segments.len()
                ));
                for entry in &thread.segments {
                    lines.push(format!("    #{:<3} {}", entry.index, entry.carried.said()));
                    lines.push(fence.opens());
                    lines.push(entry.carried.shown());
                    lines.push(fence.closes());
                }
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::reported;
    use crate::fence::Fence;

    #[test]
    fn a_room_reports_one_row_per_author_in_json() {
        let outcome = reported(
            "team",
            [
                (
                    "alice-handle".to_owned(),
                    Some(1),
                    vec![(0, 0, b"hello".as_slice())],
                ),
                ("bob-handle".to_owned(), None, vec![]),
            ],
        );
        let said = outcome.render(true, Fence::from_bytes([0; 8]));
        assert!(said.contains("\"threads\""), "{said}");
        assert!(said.contains("alice-handle"), "{said}");
        assert!(said.contains("bob-handle"), "{said}");
        assert!(said.contains("hello"), "{said}");
    }

    #[test]
    fn a_room_sections_prose_by_author_with_the_handle_outside_the_fence() {
        let outcome = reported(
            "team",
            [(
                "alice-handle-0123456789".to_owned(),
                Some(0),
                vec![(0, 0, b"hi".as_slice())],
            )],
        );
        let fence = Fence::from_bytes([0x3f; 8]);
        let said = outcome.render(false, fence);
        assert!(said.contains("`team`: 1 author(s)"), "{said}");
        assert!(said.contains("alice-handle verifies to height 0"), "{said}");
        assert!(said.contains("hi"), "{said}");
    }
}

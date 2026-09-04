// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A channel that writes to a clock rather than to a caller.
//!
//! The claim under test is the one `ARCHITECTURE.md` D-06 rules on: **an
//! endpoint with something to say and one with nothing produce the same
//! traffic.** What that reduces to, on this side of the door, is four
//! observable facts — a send does not write, a slot always does, a slot writes
//! once however often it is asked, and what a filler carries is never reported
//! as something somebody said.
//!
//! The privacy claim itself — that a host cannot tell the two apart — is not
//! made here and cannot be. It is a black-box claim about what leaves the
//! machine, and it lives in `adversary/`, which cannot reach inside.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, drops};
use kusanagi::{Cadence, Habit, Outcome, Request, Retention, Whose};

/// A slot every `seconds`.
///
/// The tests that only need "this channel has slots" take an hour, so that no
/// assertion depends on how long an ML-DSA-87 key takes to expand. The one test
/// that needs two different slots takes one second and waits.
fn every(seconds: u32) -> Habit {
    Habit {
        cadence: Cadence::Slotted {
            period: core::num::NonZeroU32::new(seconds).unwrap(),
        },
        retention: Retention::Keep,
    }
}

/// Two endpoints on one host, both writing to a one-second slot.
fn slotted(tag: &str, seconds: u32) -> (Endpoint, Endpoint, std::path::PathBuf) {
    common::pair_with(tag, every(seconds))
}

#[test]
fn a_send_on_a_slotted_channel_writes_nothing_and_says_so() {
    let (alice, _bob, host) = slotted("slot-send", 3_600);
    let before = drops(&host);

    let answer = alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: b"the money is in the second drawer".to_vec(),
        })
        .expect("a send on a slotted channel is queued, not refused");

    match answer {
        Outcome::Queued {
            waiting, period, ..
        } => {
            assert_eq!(waiting, 1);
            assert_eq!(period, Some(3_600));
        }
        other => panic!("a slotted send reported {other:?}"),
    }
    assert_eq!(
        drops(&host),
        before,
        "a send on a slotted channel reached the host, which is the leak the slot exists to close"
    );
}

#[test]
fn a_slot_writes_a_drop_whether_or_not_there_is_anything_to_say() {
    let (alice, _bob, host) = slotted("slot-filler", 3_600);
    let before = drops(&host);

    // Nothing queued, and the slot still produces exactly one drop.
    let answer = alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .expect("a tick fills its slot");
    match answer {
        Outcome::Ticked {
            wrote,
            carried,
            waiting,
            ..
        } => {
            assert_eq!(wrote, Some(0), "the first slot writes height zero");
            assert_eq!(carried, "filler");
            assert_eq!(waiting, 0);
        }
        other => panic!("a tick reported {other:?}"),
    }
    assert_eq!(
        drops(&host),
        before + 1,
        "an empty slot must cost exactly one drop, like a full one"
    );
}

#[test]
fn one_slot_takes_one_drop_however_often_it_is_asked() {
    let (alice, _bob, host) = slotted("slot-twice", 3_600);
    alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .unwrap();
    let after_first = drops(&host);

    // A scheduler that fires twice, or a person running it by hand, must not
    // produce a burst — a burst is exactly the shape a slot removes.
    let again = alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .expect("a second tick in one slot is an answer, not a failure");
    match again {
        Outcome::Ticked { wrote, carried, .. } => {
            assert_eq!(wrote, None);
            assert_eq!(carried, "nothing");
        }
        other => panic!("a repeated tick reported {other:?}"),
    }
    assert_eq!(drops(&host), after_first, "one slot produced two drops");
}

#[test]
fn a_filler_takes_a_height_and_is_never_reported_as_something_somebody_said() {
    let (alice, bob, _host) = slotted("slot-heights", 1);

    // Alice fills one slot with nothing, then queues a message for the next.
    alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .unwrap();
    alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: b"only this".to_vec(),
        })
        .unwrap();
    // A slot per second, so the next tick is a different slot.
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .unwrap();

    let heard = bob
        .run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .expect("bob reads alice's stream");

    match heard {
        Outcome::Read {
            height, segments, ..
        } => {
            assert_eq!(
                height,
                Some(1),
                "the filler must take a height: a stream that skipped one would tell a \
                 reader how many slots were empty"
            );
            let said: Vec<&kusanagi::Entry> = segments.iter().collect();
            assert_eq!(
                said.len(),
                1,
                "a filler was reported as something somebody said"
            );
        }
        other => panic!("a read reported {other:?}"),
    }
}

#[test]
fn a_tick_on_an_on_demand_channel_is_refused_by_name() {
    let (alice, _bob, _host) = common::pair("slot-ondemand");
    let refused = alice
        .run(&Request::Tick {
            name: "bob".to_owned(),
        })
        .expect_err("a channel with no period has no slot to fill");
    assert_eq!(refused.code(), "kusanagi.not_slotted");
    assert!(
        refused.render(true).contains("send --to"),
        "the refusal must name the verb that does work here"
    );
}

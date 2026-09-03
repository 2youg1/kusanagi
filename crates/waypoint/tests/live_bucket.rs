// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The one test that needs somebody else's computer.
//!
//! Conditional writes are the load-bearing assumption of this whole design, and
//! the divergences between S3-compatible hosts fail *open*. So this runs the same
//! contract every other adapter passes against a real bucket, and then asks
//! `doctor` what it found.
//!
//! **Without credentials it reports that it was skipped and returns.** It does
//! not pass quietly: a green suite that silently omitted the only test of the
//! assumption everything rests on would be worse than no test at all. Set these
//! four to run it:
//!
//! ```text
//! KUSANAGI_TEST_BUCKET     s3://ACCOUNT.r2.cloudflarestorage.com/BUCKET[/PREFIX]?region=auto
//! KUSANAGI_S3_ACCESS_KEY   the access key id
//! KUSANAGI_S3_SECRET_KEY   the secret
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code"
)]

use kusanagi_kernel::{Signer, Waypoint as _};
use kusanagi_seal::{Secret, Stream, derive};
use kusanagi_waypoint::{
    Access, Capability, Credentials, Locator, Place, Tier, Verdict, conformance, probe,
};

/// A namespace nothing else can be using, so this is safe to point at a real
/// bucket that holds real traffic.
fn namespace() -> Stream {
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).expect("no entropy");
    Secret::from_bytes(seed).stream(&Signer::from_seed(&seed).handle())
}

fn bucket() -> Option<Place> {
    let locator = std::env::var("KUSANAGI_TEST_BUCKET").ok()?;
    let access = std::env::var("KUSANAGI_S3_ACCESS_KEY").ok()?;
    let secret = std::env::var("KUSANAGI_S3_SECRET_KEY").ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let locator: Locator = locator
        .parse()
        .expect("KUSANAGI_TEST_BUCKET is not a locator");
    Some(
        Place::open(
            &locator,
            &Access {
                credentials: Some(Credentials::new(&access, &secret)),
                ..Access::default()
            },
            now,
        )
        .expect("could not open the bucket"),
    )
}

#[test]
fn a_real_bucket_satisfies_the_contract() {
    let Some(place) = bucket() else {
        eprintln!(
            "skipped: set KUSANAGI_TEST_BUCKET, KUSANAGI_S3_ACCESS_KEY and \
             KUSANAGI_S3_SECRET_KEY to run this against a real bucket"
        );
        return;
    };

    let namespace = namespace();
    conformance::run(&place, &namespace).expect("the bucket broke the waypoint contract");

    // The clause that matters: a second write to a claimed address must be
    // refused. A host that accepts it has silently stopped being write-once.
    let (address, _) = derive(&namespace, 7);
    assert_eq!(
        place.put_if_absent(&address, b"first").unwrap(),
        kusanagi_kernel::PutOutcome::Stored
    );
    assert_eq!(
        place.put_if_absent(&address, b"second").unwrap(),
        kusanagi_kernel::PutOutcome::AlreadyPresent
    );
    assert_eq!(place.get(&address).unwrap(), Some(b"first".to_vec()));
}

#[test]
fn doctor_certifies_a_real_bucket() {
    let Some(place) = bucket() else {
        eprintln!("skipped: no bucket credentials in the environment");
        return;
    };

    let certificate = probe::examine(&place, &namespace());
    for finding in certificate.findings() {
        eprintln!(
            "{:<18} {}",
            finding.capability.name(),
            finding.verdict.word()
        );
    }

    assert_eq!(
        certificate.tier(),
        Tier::WriteOnce,
        "this host does not hold write-once; kusanagi must not be run on it as it stands"
    );
    assert_eq!(
        certificate.verdict(Capability::WriteOnce),
        Some(&Verdict::Held)
    );
    // Object stores expire objects with bucket lifecycle rules rather than per
    // object, so this is expected to be a named absence rather than a failure.
    assert!(matches!(
        certificate.verdict(Capability::Expiry),
        Some(&Verdict::NotOffered { .. })
    ));
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Measuring a host instead of believing its documentation.
//!
//! What a measurement is *said in* — capability, verdict, tier, certificate — is
//! next door in `certificate.rs`. This file is only the writing and reading that
//! finds out.
//!
//! The reason this module exists is one specific way of being wrong. S3 and R2
//! refuse a second write to an occupied key when asked with `If-None-Match: *`.
//! Some compatible implementations do not: they ignore the condition and let the
//! write succeed. **The divergence fails open** — nothing errors, the protocol
//! quietly stops being write-once, and the first sign of trouble is a chain that
//! disagrees with itself weeks later.
//!
//! So an endpoint does not read a compatibility matrix. It writes twice to an
//! address it owns, reads it back, and records what happened, and the result is
//! a [`Certificate`] naming a tier that the rest of the program can act on.
//!
//! A capability that is absent is *not* a failure. A directory has no validators;
//! S3 has no per-object lifetime. Those are [`Verdict::NotOffered`], a named
//! degradation, which is a different thing from a host that claims a capability
//! and then does not hold it.

use kusanagi_kernel::{Waypoint, WaypointError};
use kusanagi_seal::{Fit, Stream, derive, seal};

use crate::certificate::{Capability, Certificate, Finding, Verdict};
use crate::conditional::{Conditional, Fetched, TtlOutcome};
use crate::conformance;

/// Measures `place` and reports what it does.
///
/// `namespace` decides the addresses used, so pointing this at a live host is
/// safe: derive it from a secret nobody else has and the probe cannot collide
/// with real traffic. Nothing here returns `Err` for a host that behaves badly —
/// misbehaviour is the *result*, not an interruption of it.
pub fn examine<P>(place: &P, namespace: &Stream) -> Certificate
where
    P: Waypoint + Conditional,
{
    let mut findings = Vec::with_capacity(Capability::ALL.len());
    findings.push(Finding {
        capability: Capability::WriteOnce,
        verdict: write_once(place, namespace),
    });

    let (conditional, validator) = conditional_read(place, namespace);
    findings.push(Finding {
        capability: Capability::ConditionalRead,
        verdict: conditional,
    });
    findings.push(Finding {
        capability: Capability::StableValidator,
        verdict: validator,
    });
    findings.push(Finding {
        capability: Capability::Expiry,
        verdict: expiry(place, namespace),
    });

    Certificate::of(findings)
}

/// The clause that decides the tier: run the whole contract every adapter signs.
fn write_once<P: Waypoint>(place: &P, namespace: &Stream) -> Verdict {
    match conformance::run(place, namespace) {
        Ok(()) => Verdict::Held,
        Err(failure) => Verdict::Broken {
            detail: failure.to_string(),
        },
    }
}

fn conditional_read<P>(place: &P, namespace: &Stream) -> (Verdict, Verdict)
where
    P: Waypoint + Conditional,
{
    let (addr, key) = derive(namespace, 100);
    let Ok(probe) = seal(&key, Fit::Veil, b"conditional-read probe") else {
        return (unsealable(), unsealable());
    };
    if let Err(error) = place.put_if_absent(&addr, &probe) {
        return (unreachable_host(&error), unreachable_host(&error));
    }

    let first = match place.get_if_changed(&addr, None) {
        Ok(fetched) => fetched,
        Err(error) => return (unreachable_host(&error), unreachable_host(&error)),
    };
    let Fetched::Fresh {
        validator: Some(validator),
        ..
    } = first
    else {
        let because = "this host does not name the versions it stores".to_owned();
        return (
            Verdict::NotOffered {
                because: because.clone(),
            },
            Verdict::NotOffered { because },
        );
    };

    let conditional = match place.get_if_changed(&addr, Some(&validator)) {
        Ok(Fetched::Unchanged) => Verdict::Held,
        Ok(Fetched::Fresh { .. }) => Verdict::NotOffered {
            because: "this host names versions but re-sends the bytes anyway".to_owned(),
        },
        Ok(Fetched::Absent) => Verdict::Broken {
            detail: "the host forgot bytes it had just stored".to_owned(),
        },
        Err(error) => unreachable_host(&error),
    };

    let stability = match place.get_if_changed(&addr, None) {
        Ok(Fetched::Fresh {
            validator: Some(again),
            ..
        }) if again == validator => Verdict::Held,
        Ok(Fetched::Fresh { .. }) => Verdict::Broken {
            detail: "the host renamed a version of bytes that did not change".to_owned(),
        },
        Ok(_) => Verdict::Broken {
            detail: "the host forgot bytes it had just stored".to_owned(),
        },
        Err(error) => unreachable_host(&error),
    };

    (conditional, stability)
}

/// Asks for a lifetime of zero seconds and reads the address afterwards.
///
/// Zero is what makes this deterministic. A probe that wrote a one-second
/// lifetime and slept would be a test whose result depends on how busy the
/// machine was; a probe that trusted the host's acknowledgement would be reading
/// documentation again.
fn expiry<P>(place: &P, namespace: &Stream) -> Verdict
where
    P: Waypoint + Conditional,
{
    let (addr, key) = derive(namespace, 101);
    let Ok(probe) = seal(&key, Fit::Veil, b"expiry probe") else {
        return unsealable();
    };
    match place.put_with_ttl(&addr, &probe, 0) {
        Ok(TtlOutcome::NotOffered) => Verdict::NotOffered {
            because: "this host has no per-object lifetime; sweep with a bucket rule instead"
                .to_owned(),
        },
        Ok(TtlOutcome::Accepted) => match place.get(&addr) {
            Ok(None) => Verdict::Held,
            Ok(Some(_)) => Verdict::Broken {
                detail: "the host accepted a lifetime of zero and served the object anyway"
                    .to_owned(),
            },
            Err(error) => unreachable_host(&error),
        },
        Err(error) => unreachable_host(&error),
    }
}

/// A probe that could not be sealed, which a fixed-size envelope around a
/// short constant never is; named so the verdict is honest if it ever were.
fn unsealable() -> Verdict {
    Verdict::Broken {
        detail: "this build could not seal a probe the size of a drop".to_owned(),
    }
}

fn unreachable_host(error: &WaypointError) -> Verdict {
    Verdict::Broken {
        detail: error.to_string(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::examine;
    use crate::certificate::{Capability, Tier, Verdict};
    use crate::{Locator, Place};
    use kusanagi_kernel::{Signer, Waypoint as _};
    use kusanagi_seal::Secret;
    use std::str::FromStr as _;

    fn namespace(tag: u8) -> kusanagi_seal::Stream {
        Secret::from_bytes([tag; 32]).stream(&Signer::from_seed(&[tag; 32]).handle())
    }

    #[test]
    fn a_directory_holds_write_once_and_offers_nothing_else() {
        let root = std::env::temp_dir().join(format!("kusanagi-probe-{}", std::process::id()));
        let place = Place::open(
            &Locator::from_str(&root.display().to_string()).unwrap(),
            &crate::access::Access::default(),
            0,
        )
        .unwrap();

        let certificate = examine(&place, &namespace(3));
        assert_eq!(certificate.tier(), Tier::WriteOnce);
        assert!(!certificate.any_broken());
        assert_eq!(
            certificate.verdict(Capability::WriteOnce),
            Some(&Verdict::Held)
        );
        assert!(matches!(
            certificate.verdict(Capability::ConditionalRead),
            Some(&Verdict::NotOffered { .. })
        ));
        assert!(matches!(
            certificate.verdict(Capability::Expiry),
            Some(&Verdict::NotOffered { .. })
        ));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_host_that_overwrites_is_reported_broken_not_ignored() {
        let certificate = examine(&Overwriting::default(), &namespace(4));
        assert_eq!(certificate.tier(), Tier::AckFirstSeen);
        assert!(certificate.any_broken());
        assert!(matches!(
            certificate.verdict(Capability::WriteOnce),
            Some(&Verdict::Broken { .. })
        ));
    }

    /// The host this whole module exists because of: it takes the conditional
    /// write, ignores the condition, and reports success.
    #[derive(Default)]
    struct Overwriting {
        drops: std::sync::Mutex<std::collections::BTreeMap<kusanagi_kernel::DropAddr, Vec<u8>>>,
    }

    impl kusanagi_kernel::Waypoint for Overwriting {
        fn put_if_absent(
            &self,
            addr: &kusanagi_kernel::DropAddr,
            bytes: &[u8],
        ) -> Result<kusanagi_kernel::PutOutcome, kusanagi_kernel::WaypointError> {
            self.drops.lock().unwrap().insert(*addr, bytes.to_vec());
            Ok(kusanagi_kernel::PutOutcome::Stored)
        }

        fn get(
            &self,
            addr: &kusanagi_kernel::DropAddr,
        ) -> Result<Option<Vec<u8>>, kusanagi_kernel::WaypointError> {
            Ok(self.drops.lock().unwrap().get(addr).cloned())
        }

        fn delete(
            &self,
            addr: &kusanagi_kernel::DropAddr,
        ) -> Result<(), kusanagi_kernel::WaypointError> {
            self.drops.lock().unwrap().remove(addr);
            Ok(())
        }
    }

    impl crate::Conditional for Overwriting {
        fn get_if_changed(
            &self,
            addr: &kusanagi_kernel::DropAddr,
            _known: Option<&crate::Validator>,
        ) -> Result<crate::Fetched, kusanagi_kernel::WaypointError> {
            Ok(self
                .get(addr)?
                .map_or(crate::Fetched::Absent, |bytes| crate::Fetched::Fresh {
                    bytes,
                    validator: None,
                }))
        }

        fn put_with_ttl(
            &self,
            addr: &kusanagi_kernel::DropAddr,
            bytes: &[u8],
            _seconds: u64,
        ) -> Result<crate::TtlOutcome, kusanagi_kernel::WaypointError> {
            self.put_if_absent(addr, bytes)?;
            Ok(crate::TtlOutcome::NotOffered)
        }
    }
}

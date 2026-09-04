// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Windows: every file and directory carries its own protected access list.
//!
//! **This is the one module in the workspace that contains `unsafe`**, and the
//! whole of its job is to hand the operating system a security descriptor. Four
//! calls, each wrapped alone, each with the reason its pointers are valid written
//! above it. The root `Cargo.toml` allowlist names this module and nothing else;
//! a second address for the exception is a review failure.
//!
//! **Why not a crate.** The two published wrappers for this API
//! (`windows-acl`, `windows-permissions`) were last touched in 2019 and 2020,
//! and this code sits on the permission path of a security product. Vendoring
//! the `unsafe` into a dependency does not remove it; it moves it somewhere
//! nobody in this repository reads. `windows-sys` is Microsoft's own binding and
//! adds no new vendor.
//!
//! **Why `CreateDirectoryW` and not `SetNamedSecurityInfoW`.** The second takes
//! a path and resolves it, so a junction planted where the site is about to go
//! would redirect the change onto somebody else's directory. `CreateDirectoryW`
//! fails when the name already exists — and a junction counts as existing — so
//! this program only ever sets a descriptor on a directory it just made.
//!
//! **Known limit.** These are the plain Win32 entry points, so a path over the
//! legacy 260-character limit is refused by the operating system rather than
//! silently truncated. It arrives as [`SiteError::Local`] carrying the real
//! error, and the answer is a shorter `--root`.

#![expect(
    unsafe_code,
    reason = "the one module allowed to call Win32 directly; its entire job is \
              to hand the operating system a security descriptor. Root Cargo.toml \
              allowlist, entry three."
)]

use std::ffi::OsStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::FromRawHandle as _;
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
};
use windows_sys::Win32::System::Memory::{VirtualLock, VirtualUnlock};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

use zeroize::Zeroize as _;

use crate::error::SiteError;

/// The access list every file and directory of a site is created with.
///
/// `D:P` is a discretionary list that **refuses inheritance**, which is the
/// whole point: a site under a directory somebody opened up must not be open.
/// `OICI` makes it apply to what is created inside. Two entries, full access:
///
/// - `OW` is OWNER RIGHTS (`S-1-3-4`), which is this account, because this
///   account creates the file. Naming the owner rather than a fixed SID means
///   nothing here has to ask who is running.
/// - `SY` is `SYSTEM`, without which backup, indexing and the update mechanism
///   fail in ways nobody connects to this program.
///
/// **Administrators are deliberately not named.** They can take ownership of
/// anything on the machine, so listing them protects against nobody, and leaving
/// them out keeps the list short enough to read in a test.
const SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)";

/// A security descriptor allocated by Win32, freed when this is dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    /// Builds the descriptor [`SDDL`] describes.
    fn of_this_account() -> Result<Self, SiteError> {
        let text = wide(OsStr::new(SDDL));
        let mut raw: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `text` is a NUL-terminated UTF-16 buffer that outlives this
        // call, `raw` is a live local written at most once by the callee, and a
        // null size pointer is documented as "do not report the size".
        let built = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &raw mut raw,
                std::ptr::null_mut(),
            )
        };
        if built == 0 {
            return Err(SiteError::Permissions {
                what: "build the access list a site needs",
                source: std::io::Error::last_os_error(),
            });
        }
        Ok(Self(raw))
    }

    /// The attributes to create something with, borrowing this descriptor.
    ///
    /// The returned value points into `self`, so it must not outlive it. Every
    /// caller here builds it, uses it and drops it inside one function.
    fn attributes(&self) -> Result<SECURITY_ATTRIBUTES, SiteError> {
        let length = u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(|_| {
            SiteError::Permissions {
                what: "describe the access list to the operating system",
                source: std::io::Error::other("SECURITY_ATTRIBUTES does not fit in a u32"),
            }
        })?;
        Ok(SECURITY_ATTRIBUTES {
            nLength: length,
            lpSecurityDescriptor: self.0,
            // A child process must not inherit a handle to a channel secret.
            bInheritHandle: 0,
        })
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: the pointer came from
        // `ConvertStringSecurityDescriptorToSecurityDescriptorW`, which
        // documents `LocalFree` as the way to release it. `Descriptor` owns it,
        // is not `Copy`, and hands out no copies, so this runs exactly once.
        unsafe { LocalFree(self.0) };
    }
}

/// Creates `path` and every missing parent, each with its own protected list.
///
/// A parent that already exists keeps the list it has, exactly as a directory
/// this build did not create keeps its mode on Unix. Everything inside is
/// closed regardless.
pub(super) fn create_dir(path: &Path, action: &'static str) -> Result<(), SiteError> {
    let descriptor = Descriptor::of_this_account()?;
    let attributes = descriptor.attributes()?;
    // Shallowest first, so a parent exists before its child is asked for.
    let mut missing: Vec<&Path> = path
        .ancestors()
        .take_while(|ancestor| !ancestor.as_os_str().is_empty() && !ancestor.exists())
        .collect();
    missing.reverse();
    for directory in missing {
        let name = wide(directory.as_os_str());
        // SAFETY: `name` is a NUL-terminated UTF-16 path that outlives the call,
        // and `attributes` is a live local whose descriptor pointer is owned by
        // `descriptor`, which outlives this loop.
        let made = unsafe { CreateDirectoryW(name.as_ptr(), &raw const attributes) };
        if made == 0 {
            let refused = std::io::Error::last_os_error();
            // Somebody else created it between the check and the call. It is
            // theirs, so it keeps their list, and the files inside are still
            // this program's.
            let code = refused
                .raw_os_error()
                .and_then(|raw| u32::try_from(raw).ok());
            if code != Some(ERROR_ALREADY_EXISTS) {
                return Err(SiteError::Local {
                    action,
                    source: refused,
                });
            }
        }
    }
    Ok(())
}

/// Creates a file that must not already exist, with its own protected list.
///
/// `CREATE_NEW` refuses an existing name — including a junction or a symbolic
/// link — rather than following it, which is what makes this safe on a path
/// somebody else can reach.
///
/// `CreateFileW` directly, because `OpenOptions` has no stable way to carry a
/// security descriptor: `OpenOptionsExt::security_attributes` is unstable, and
/// creating the file first and re-permissioning it afterwards is the path-based
/// operation this whole module exists to avoid.
///
/// Sharing is denied outright — the third argument is zero — because nothing
/// should be reading a channel secret while it is being written.
pub(super) fn create_file(path: &Path, action: &'static str) -> Result<File, SiteError> {
    let descriptor = Descriptor::of_this_account()?;
    let attributes = descriptor.attributes()?;
    let name = wide(path.as_os_str());
    // SAFETY: `name` is a NUL-terminated UTF-16 path that outlives the call,
    // `attributes` is a live local whose descriptor is owned by `descriptor`,
    // and a null template handle is documented as "no template".
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_GENERIC_WRITE,
            0,
            &raw const attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(SiteError::Local {
            action,
            source: std::io::Error::last_os_error(),
        });
    }
    // SAFETY: `handle` was just returned by `CreateFileW`, is not the invalid
    // value, and no copy of it exists anywhere. `File` takes sole ownership and
    // closes it exactly once.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// A NUL-terminated UTF-16 copy of `text`, as every `…W` entry point wants.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Seals `plain` so that only this Windows account can open it.
///
/// `CRYPTPROTECT_UI_FORBIDDEN` because a one-shot command must never stop for a
/// dialogue; the extra entropy binds a blob to this program, so a blob lifted
/// out of a site does not open under another application's call.
pub(crate) fn protect(plain: &[u8]) -> Result<Vec<u8>, SiteError> {
    crypt(plain, Direction::Seal)
}

/// Opens what [`protect`] sealed, under the account that sealed it.
pub(crate) fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, SiteError> {
    crypt(sealed, Direction::Open)
}

/// Which way through DPAPI.
#[derive(Clone, Copy)]
enum Direction {
    Seal,
    Open,
}

/// What binds a blob to this program as well as to this account.
///
/// A constant rather than a secret: it is not a key and does not pretend to be
/// one. What it buys is that a blob copied out of a site does not open under
/// somebody else's `CryptUnprotectData` call in the same session.
const ENTROPY: &[u8] = b"kusanagi/site/1";

/// One call through DPAPI, in either direction.
fn crypt(input: &[u8], direction: Direction) -> Result<Vec<u8>, SiteError> {
    let refused = |what: &'static str| SiteError::Permissions {
        what,
        source: std::io::Error::last_os_error(),
    };
    let source = blob(input)?;
    let entropy = blob(ENTROPY)?;
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: both input blobs point at slices that outlive the call, `out` is a
    // live local the callee writes once, and the two null pointers are the
    // documented values for "no reserved data" and "no prompt".
    let done = unsafe {
        match direction {
            Direction::Seal => CryptProtectData(
                &raw const source,
                std::ptr::null(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut out,
            ),
            Direction::Open => CryptUnprotectData(
                &raw const source,
                std::ptr::null_mut(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut out,
            ),
        }
    };
    if done == 0 {
        return Err(refused(match direction {
            Direction::Seal => "seal a site record for this account",
            Direction::Open => "open a site record sealed for this account",
        }));
    }
    let len = usize::try_from(out.cbData)
        .map_err(|_| refused("read back a blob larger than this machine can address"))?;
    // SAFETY: on success DPAPI reports a buffer of exactly `cbData` bytes at
    // `pbData`, allocated with `LocalAlloc`. The copy happens before the free,
    // and nothing else holds the pointer.
    let plaintext = unsafe { std::slice::from_raw_parts_mut(out.pbData, len) };
    // DPAPI chose this buffer, so the plaintext of a record exists for a moment
    // in a page this program did not lock. Pinning it here closes that moment;
    // erasing it below closes what the free would otherwise leave behind.
    lock(plaintext);
    let bytes = plaintext.to_vec();
    if matches!(direction, Direction::Open) {
        plaintext.zeroize();
    }
    unlock(plaintext);
    // SAFETY: `out.pbData` came from DPAPI, which documents `LocalFree` as the
    // way to release it, and this is the only release of it.
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(bytes)
}

/// A blob pointing at `bytes`, which the caller keeps alive across the call.
///
/// It borrows rather than owns: nothing here allocated the bytes and nothing
/// here frees them, so the blob is valid exactly as long as the slice is.
fn blob(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, SiteError> {
    let len = u32::try_from(bytes.len()).map_err(|_| SiteError::Permissions {
        what: "describe a site record to the operating system",
        source: std::io::Error::other("a record is larger than four gigabytes"),
    })?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: len,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

/// Pins `bytes` in physical memory, so the page file never sees them.
///
/// **What this buys and what it does not.** A site record is a channel secret;
/// once it is in a page the operating system may evict, it is in `pagefile.sys`,
/// in a hibernation image, and in whatever a backup of those files reached.
/// `VirtualLock` keeps the page resident, so those three copies stop being made.
/// It does nothing about a crash dump, which is machine-wide policy, and nothing
/// about values *derived* from a record afterwards — an expanded signing key
/// lives in an ordinary page and is erased rather than pinned. `site-SPEC.md` §9
/// records that boundary.
///
/// **A failure is not reported.** The lock is a hardening measure over a
/// correctness-neutral property: a record that could not be pinned is still the
/// right record, and refusing to read it would turn a tightened working-set
/// quota into an endpoint that cannot open its own channels. What is done
/// instead is raising the quota once, below, so the common cause cannot arise.
pub(super) fn lock(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    room_to_lock();
    // SAFETY: the pointer and length describe a live slice the caller owns for
    // the duration of this call, which is all `VirtualLock` reads. Its return is
    // deliberately dropped; the doc comment above says why.
    let _locked = unsafe { VirtualLock(bytes.as_ptr().cast(), bytes.len()) };
}

/// Releases what [`lock`] pinned. Call it *after* erasing, never before.
pub(super) fn unlock(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    // SAFETY: same slice, same lifetime, and unlocking a range that was never
    // locked is a documented no-op that reports failure rather than misbehaving.
    let _unlocked = unsafe { VirtualUnlock(bytes.as_ptr().cast(), bytes.len()) };
}

/// Raises this process's minimum working set once, so locking can succeed.
///
/// `VirtualLock` is bounded by the *minimum* working set size, which Windows
/// defaults low enough that a site with a few dozen channels would reach it.
/// Eight mebibytes is far above anything this program holds at once and far
/// below anything a desktop would notice.
fn room_to_lock() {
    static RAISED: std::sync::Once = std::sync::Once::new();
    RAISED.call_once(|| {
        const MINIMUM: usize = 8 * 1_024 * 1_024;
        const MAXIMUM: usize = 32 * 1_024 * 1_024;
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no
        // release and is valid for the life of the process; the two sizes are
        // plain integers. A refusal leaves the default quota in place, which
        // `lock` already treats as acceptable.
        let _raised = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), MINIMUM, MAXIMUM) };
    });
}

# Third-party dependencies

Every crate kusanagi links into its shipped binary, why it is there, and its
licence. Regenerate the tree with `cargo tree --workspace --edges normal`; check
the licences with `cargo deny check` against `deny.toml`.

**Direct dependencies: twelve.** Everything else below is something one of those
twelve pulled in.

| Crate | Why it is here | Licence |
|---|---|---|
| `blake3` | the workspace's one hash: content addresses, key derivation, ETags | CC0-1.0 / Apache-2.0 |
| `ed25519-dalek` | segment and grant signatures | BSD-3-Clause |
| `chacha20poly1305` | sealing a segment under a single-use key | Apache-2.0 / MIT |
| `getrandom` | the one source of entropy, straight from the operating system | Apache-2.0 / MIT |
| `zeroize` | erasing the channel secret, the stream and the per-drop key when they go out of scope. **Already in the tree** beneath `ed25519-dalek`, which zeroizes its signing key, so depending on it directly added nothing to audit | Apache-2.0 / MIT |
| `hmac`, `sha2` | AWS SigV4 request signing, which is not ours to choose | Apache-2.0 / MIT |
| `ureq` | a blocking HTTP client, so no async runtime enters the binary | Apache-2.0 / MIT |
| `thiserror` | `Display` and `From` for typed errors; no runtime footprint | Apache-2.0 / MIT |
| `clap` | argument parsing, at the edge of the binary only | Apache-2.0 / MIT |
| `serde`, `serde_json` | `--json` output. **Never** used for anything hashed or signed | Apache-2.0 / MIT |
| `proptest` (dev) | sampling the attenuation property over arbitrary chains | Apache-2.0 / MIT |

**Not used, deliberately.** No async runtime. No `hkdf` — BLAKE3 has a key
derivation mode, and one hash construction is enough to audit. No `hex` — the
network has exactly one text encoding and it is thirty lines. No AWS SDK — the
whole S3 surface used here is two requests and a signature. No date library — the
only date arithmetic is one timestamp format.

## The transitive tree

`ring` and `rustls` arrive through `ureq` and are what make an HTTPS bucket
reachable; they are the largest thing in the tree by far. `curve25519-dalek`
arrives through `ed25519-dalek`. The rest are small support crates:
`aead`, `anstream`, `anstyle`, `anstyle-parse`, `anstyle-query`, `anstyle-wincon`,
`arrayvec`, `base64`, `block-buffer`, `bytes`, `cfg-if`, `chacha20`, `cipher`,
`clap_builder`, `clap_derive`, `clap_lex`, `colorchoice`, `constant_time_eq`,
`cpufeatures`, `crypto-common`, `curve25519-dalek-derive`, `digest`, `ed25519`,
`generic-array`, `heck`, `http`, `httparse`, `inout`, `is_terminal_polyfill`,
`itoa`, `log`, `memchr`, `once_cell`, `once_cell_polyfill`, `opaque-debug`,
`percent-encoding`, `poly1305`, `proc-macro2`, `quote`, `rustls-pki-types`,
`rustls-webpki`, `serde_core`, `serde_derive`, `signature`, `strsim`, `subtle`,
`syn`, `typenum`, `ureq-proto`, `universal-hash`, `unicode-ident`, `untrusted`,
`utf8parse`, `webpki-roots`, `zerocopy`, `zeroize_derive`, plus platform shims
for Windows and wasi.

Everything in the tree is Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC,
CC0-1.0, Zlib, Unicode-3.0, or — for `webpki-roots`, which is Mozilla's CA
certificate set and therefore data rather than code — CDLA-Permissive-2.0. No
copyleft licence appears, and none is permitted.

`cargo deny check` is green against `deny.toml`, which is what enforces the list
above rather than this paragraph. Two duplicate versions are tolerated as
warnings today (`getrandom`, `windows-sys`, `syn`, `cpufeatures`), each because
two dependencies pin different majors; none of them reaches our own code.

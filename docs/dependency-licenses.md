# Reviewed desktop dependency license exceptions

Review date: 18 October 2026. The repository owner approved narrowly scoped
desktop dependency exceptions. This does not change Brandon's AGPL-3.0-or-later
license, approve arbitrary new licenses, or certify a binary distribution's full
license compliance. Retain upstream notices and license texts, and review source
availability/attribution requirements before shipping redistributable bundles.

The default-deny policy remains in place. Rust exceptions in `deny.toml` match
an exact crate version; npm exceptions in `scripts/check-npm-licenses.mjs` match
the exact package name, version and reported SPDX expression. A version change,
changed expression or another package using the same license requires a new
review. CI still checks unknown/missing/mismatched license metadata.

## Rust

| Crate | Exact version | Declared license | Desktop dependency path |
| --- | --- | --- | --- |
| cssparser | 0.37.0 | MPL-2.0 | Tauri HTML/CSS utilities |
| cssparser-macros | 0.7.1 | MPL-2.0 | cssparser |
| dtoa-short | 0.3.5 | MPL-2.0 | CSS number serialization |
| selectors | 0.38.0 | MPL-2.0 | Tauri HTML/CSS utilities |
| option-ext | 0.2.0 | MPL-2.0 | Tauri directory discovery |
| target-lexicon | 0.12.16 | Apache-2.0 WITH LLVM-exception | Native platform build tooling |

Metadata was checked against the exact published crate manifests resolved by
`Cargo.lock`; authoritative license texts are linked below. Registry records:
[cssparser](https://crates.io/crates/cssparser/0.37.0),
[cssparser-macros](https://crates.io/crates/cssparser-macros/0.7.1),
[dtoa-short](https://crates.io/crates/dtoa-short/0.3.5),
[selectors](https://crates.io/crates/selectors/0.38.0),
[option-ext](https://crates.io/crates/option-ext/0.2.0),
[target-lexicon](https://crates.io/crates/target-lexicon/0.12.16).

Authoritative texts: [Mozilla MPL 2.0](https://www.mozilla.org/en-US/MPL/2.0/),
[SPDX LLVM exception](https://spdx.org/licenses/LLVM-exception.html).
Exceptions use [cargo-deny's per-package mechanism](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html#exceptions),
not a global MPL/LLVM allow-list expansion. No advisory ignore is added.

## Production npm

| Package | Exact version | Reported license | Recorded selection |
| --- | --- | --- | --- |
| @tauri-apps/api | 2.12.3 | Apache-2.0 OR MIT | MIT alternative |
| @tauri-apps/plugin-dialog | 2.8.2 | MIT OR Apache-2.0 | MIT alternative |
| @ungap/structured-clone | 1.4.0 | ISC | Exact transitive dependency exception |

The two Tauri packages provide an MIT choice; the policy does not approve every
possible SPDX expression containing the word MIT. The structured-clone shim is
pulled into the Markdown frontend. Versions and expressions were checked with
the pinned pnpm production license inventory and installed package manifests.

Upstream records: [Tauri API](https://www.npmjs.com/package/@tauri-apps/api/v/2.12.3),
[Tauri dialog](https://www.npmjs.com/package/@tauri-apps/plugin-dialog/v/2.8.2),
[structured-clone](https://www.npmjs.com/package/@ungap/structured-clone/v/1.4.0).

Regression tests reject unreviewed versions, mixed-version inventories, unrelated
packages, changed AND/OR expressions, missing identities and unknown licenses.

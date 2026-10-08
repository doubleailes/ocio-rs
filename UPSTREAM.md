# Upstream reference snapshot

This file records which revision of the C++ reference implementation
([AcademySoftwareFoundation/OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO))
ocio-rs is a port of, and what remains to reach the latest release.
Update it whenever upstream changes are ported.

## Current snapshot

| | Tag / branch | Commit | Date |
|---|---|---|---|
| **Ported up to** | `main` (2.6.0 dev) | [`93878f9a8630081a6e8d866acf9fd119400c7a4e`](https://github.com/AcademySoftwareFoundation/OpenColorIO/commit/93878f9a8630081a6e8d866acf9fd119400c7a4e) — *Add RGB-to-HMJ FixedFunction (#2336)* | 2026-09-16 |
| Previous release | `v2.5.0` | [`592a122b37467c3376f2387fbbdbe7b571fad39f`](https://github.com/AcademySoftwareFoundation/OpenColorIO/commit/592a122b37467c3376f2387fbbdbe7b571fad39f) | 2025-10-01 |
| **Target release** | `v2.6.0` | [`10a43ec1df03d072ca9a9e2fb3f26e454b0900d8`](https://github.com/AcademySoftwareFoundation/OpenColorIO/commit/10a43ec1df03d072ca9a9e2fb3f26e454b0900d8) | 2026-09-30 |

The port therefore sits **between `v2.5.0` and `v2.6.0`**: every behavioral
change of the 2.6 cycle up to and including `93878f9` is ported (see
[Ported from the 2.6 cycle](#ported-from-the-26-cycle)), and the release-week
commits after it are not (see [Missing for v2.6.0](#missing-for-v260)).

Facts that pin the snapshot:

* Every file under `tests/data/files` that changed between `v2.5.0` and
  `v2.6.0` is byte-identical to its `v2.6.0` version (SMPTE ST 2136-1 CLF
  files, cycle-detection `.ctf` files, ...).
* `LAST_SUPPORTED_MINOR_VERSION` is `[0, 6]` (config version 2.6 is accepted)
  and CTF version 2.6 is current (needed by HMJ, #2336).
* The built-in configs stop at `v4.0.0_aces-v2.0_ocio-v2.5` (the `v5.0.0` ones
  arrive in #2353, after the snapshot).
* `OCIO_VERSION` in `src/lib.rs` is `"2.5.2"`: the latest release whose
  library changes are all ported (see [2.5.x patch releases](#25x-patch-releases)).
  Upstream `main` already reports `2.6.0` (since #2266, 2026-03-04), but the
  port must not claim 2.6.0 until the commits listed below are ported.

### 2.5.x patch releases

`v2.5.1` ([`004f800`](https://github.com/AcademySoftwareFoundation/OpenColorIO/commit/004f80009653fa322c1133ba2c57b8ed526599c9))
and `v2.5.2` ([`c52966a`](https://github.com/AcademySoftwareFoundation/OpenColorIO/commit/c52966a6677723d5bd2dbef0ccec3fed9cbc3790))
live on a release branch, not on `main`. Their library changes are all
cherry-picks of `main` PRs already ported (#2206, #2224, #2227, #2231, #2204,
#2270, #2276, #2281, #2307, #2308), plus two branch-only commits that are also
covered: `08a51c6` (GradingTone `MaxSCTol = MaxSC + Error`, from #2282) and
`b27be86` (C++ `#include` build fix, N/A). Nothing from 2.5.x is missing.

## Missing for v2.6.0

Upstream commits between the snapshot and `v2.6.0` (`git log 93878f9..v2.6.0`):

| Commit | PR | Title | Status in ocio-rs |
|---|---|---|---|
| `2e1c9ac` | #2350 | Support Yaml 0.9.0 and other build fixes | N/A (C++ build, expat exception trampolines, yaml-cpp test strictness) |
| `eb25aaa` | #2344 | Config compatibility validation for HDR displays | **Not ported**: `ConfigCompatibility` / `CheckCompatibility` (`CONFIG_HDR_DISPLAY_SUPPORT_26`), the `ociocheck` "Compatibility" section and its `-i` / `-v` options |
| `e7b7461` | #2354 | Fix MSVC mutex initialization | N/A |
| `1f0ca55` | #2349 | SMPTE CLF no-clamp Range writing | **Not ported**: `GetEquivalentNoClampRange`. The CLF writer must emit `<Range style="noClamp">` for a leading/trailing scale-offset matrix matching the 64/1023–940/1023 video range |
| `43f2c67` | #2341 | Display and view aliases | **Not ported**: view / shared-view `aliases`, `use_display_aliases`, canonical display/view names, `getResolvedDisplayViewColorSpaceName`, `getDisplayDescription`, alias resolution in `DisplayViewTransform` / `LegacyViewingPipeline` / `GetProcessorFromConfigs` / `isColorSpaceUsed`, 2.6 version checks |
| `d8bc9fd` | #2345 | Color interop ID functions | **Not ported**: `locateBuiltinColorSpace`, `findColorSpaceForID`, `generateLocalIDForColorSpace` (Annex C sanitising, reserved config names). The fingerprint code in `src/apphelpers/merge_configs/merge_utils.rs` can be reused |
| `a537b13` | #2353 | 2.6.0 built-in configs | **Not ported**: `cg-config-v5.0.0_aces-v2.1_ocio-v2.6` and `studio-config-v5.0.0_aces-v2.1_ocio-v2.6`, default / latest URIs moved to v5.0.0. **Depends on #2341** (the new configs use `use_display_aliases: true`) |
| `8ca3fb3` | #2319 | GPU shader correctness and DXC warnings | **Not ported**: HUE-FX inverse `knStartY` offset in the RGB curve shader (an output fix for the GPU inverse hue curve), `GpuShaderText::intCast` (C-style casts in HLSL), dead `SinCos` call in the ACES2 gamut compress shader, 2-argument inverse eval signature |
| `74cbe93` | #2322 | Remove ocioview | N/A |
| `22dd9c4` | #2355 | 2.6 release notes | N/A |
| `10a43ec` | — | Set library version | Bump `OCIO_VERSION` (see above) |

Suggested order: #2341 → #2353 → #2345 → #2344 → #2349 → #2319, then
`OCIO_VERSION = "2.6.0"`.

## Known deviations inside the snapshot

Commits before the snapshot that are ported with a known difference from C++:

* **#2244 (`267c938`), optimizer debugging.** The optimizer behavior (pass
  loop counting replaced ops, stack-based inverse removal) is ported. The
  debug logging (per-step "N optimisations found", per-pass summaries,
  `SerializeOpVec` op dumps, max-passes warning) is not.
* **#2318 (`7063751`), cycle guard in `BuildOps`.** `src/transforms/build.rs`
  allows 64 nested levels (C++: 33) and its message reads "...building ops for
  transforms." (C++: "...building ops from transform.").
* **#2265 (`eaa0281`), CLF id generation.** For a process list without an id,
  `generated_id` in `src/fileformats/ctf/writer.rs` hashes the op data with
  MD5. C++ uses `CacheIDHashUUID(ops.getCacheID())` (XXH3-128 over the ops
  cache id). The `urn:uuid:` format matches, but the values differ.

## Ported from the 2.6 cycle

Behavioral commits from `v2.5.0..93878f9` that are already in ocio-rs. Commits
touching only CI, docs, Python bindings, or C++-only GPU test backends
(Vulkan, DirectX 12) are omitted.

| Commit | PR | Title |
|---|---|---|
| `5d2409b` | #2224 | Remove [0,1] clamping from ICC transforms |
| `d7917d4` | #2231 | Heap-use-after-free in `ThrowInvalidRegex` (error text) |
| `449d1f2` | #2227 | ACES2 table subscript out of range (extra upper entry) |
| `6a1dff6` | #2204 | Interop ID issue in `ociocheck` |
| `eaa0281` | #2265 | Support for SMPTE ST 2036-1 compliant CLF files (written as ST 2136-1:2024; see deviation above) |
| `e9c4871` | #2270 | Vector comparison expression for HLSL |
| `d9eb50c` | #2197 | Alpha channel defaults to 1 |
| `4a119df` | #2281 | OpenGL ES type issues in ACES2 FixedFunction shaders |
| `044f66b` | #2276 | Hue curve parameters / CTF description `language` attribute |
| `222ca50` | #2307 | `sscanf` field widths in LUT readers |
| `c173846` | #2308 | Miscellaneous hardening (LUT size limits, cache id padding, CustomKeys, ...) |
| `7063751` | #2318 | Avoid cycles in transforms (see deviation above) |
| `f660cee` | #2310 | `CreateFromFile` missing-file errors |
| `5a808fb` | #2323 | `ocioconvert` invalid bit-depth handling |
| `ac2cf78` | #2342 | RGB / hue curve performance with MSL |
| `3f96f20` | #2343 | Apple Log 2 built-in transform |
| `267c938` | #2244 | Optimizer pass structure (see deviation above) |
| `93878f9` | #2336 | RGB-to-HMJ FixedFunction |

## Updating

1. Pick the new target (`git ls-remote --tags https://github.com/AcademySoftwareFoundation/OpenColorIO`).
2. List the work: `git log --reverse 93878f9..<target> -- src include tests`
   (replace `93878f9` with the "Ported up to" commit above).
3. Port each commit in order, including its C++ unit tests, and copy changed
   files from upstream `tests/data/files` byte for byte.
4. Update this file: the snapshot table, the missing / deviation lists and,
   when a release tag is reached, `OCIO_VERSION` in `src/lib.rs`.

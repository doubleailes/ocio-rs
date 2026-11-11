# ocio-rs

A pure Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
(OCIO), the color management framework for visual effects and animation.

The crate is named `ocio`. It has no C++ dependency, and its CPU renderers
reproduce the results of OCIO built with SSE2 on every target.

## Status

The current release is `ocio` **0.3.0**. The port follows OCIO `main`
between `v2.5.0` and `v2.6.0` (up to upstream commit `93878f9`, 2026-09-16),
and the library reports version `2.5.2`: every library change of the 2.5.x
patch releases is ported. See [`UPSTREAM.md`](UPSTREAM.md) for the exact
upstream revision and [`PORTING.md`](PORTING.md) for the architecture and the
numerics notes.

### Done

* **Configs:** YAML parsing and writing, contexts and environment variables,
  file rules, viewing rules, looks, named transforms, virtual displays,
  built-in configs (up to `v4.0.0_aces-v2.0_ocio-v2.5`), OCIOZ archives,
  processor caching and config merging.
* **Transforms and ops:** the OCIO transform set (including the ACES 2
  fixed functions, Apple Log 2 and RGB-to-HMJ), op optimization, and a CPU
  processor working on interleaved RGBA `f32`.
* **CPU numerics:** the CPU renderers reproduce OCIO built with SSE2 (the way
  OCIO ships), emulated in portable Rust so the results are the same on every
  target, ARM included. They were checked bit for bit against PyOpenColorIO
  2.6.0 on the built-in configs and the gamma, log, CDL, PQ, exposure /
  contrast and grading ops.
* **File formats:** the LUT and transform formats OCIO reads and writes
  (CLF/CTF including SMPTE ST 2136-1, `.cube`, `.spi1d`, `.spi3d`, `.3dl`,
  ...), and LUT baking.
* **App helpers:** color space / display / view menus, the legacy viewing
  pipeline and mixing helpers.
* **GPU shader text generation** for GLSL, GLSL ES, Vulkan GLSL, HLSL, MSL,
  Cg and OSL, plus **WGSL** (an ocio-rs addition for `wgpu` / WebGPU, see the
  `ocio::gpu::wgsl` module). All 270 OpenColorIO GPU unit tests are ported and
  run on the WGSL shaders through naga, and the
  [`wgpu-example`](wgpu-example/README.md) program validates a full ACES 2.0
  display transform on a real GPU against the CPU processor. ocio-rs has no
  GPU execution backend of its own: the shaders are meant for your renderer.
* **Command line tools:** ports of the OCIO applications (see below).

### Missing for OCIO v2.6.0

Upstream commits after the snapshot that are not ported yet (details and the
suggested order in [`UPSTREAM.md`](UPSTREAM.md#missing-for-v260)):

* Display and view aliases (#2341).
* The 2.6.0 built-in configs, `v5.0.0_aces-v2.1_ocio-v2.6` (#2353, needs
  #2341).
* Color interop ID functions (#2345).
* Config compatibility validation for HDR displays and the `ociocheck`
  "Compatibility" section (#2344).
* SMPTE CLF no-clamp `Range` writing (#2349).
* GPU shader correctness fixes (#2319).

Once these are ported, the library version moves to `2.6.0`. A few known,
documented deviations from the C++ code (optimizer debug logging, the cycle
guard depth, CLF generated ids) are listed in
[`UPSTREAM.md`](UPSTREAM.md#known-deviations-inside-the-snapshot).

## Usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ocio = "0.3"
```

```rust
use ocio::Config;

let config = Config::create_from_file("config.ocio").unwrap();
let processor = config.get_processor("ACEScg", "sRGB - Display").unwrap();
let cpu = processor.default_cpu_processor();
let mut pixel = [0.18f32, 0.18, 0.18];
cpu.apply_rgb(&mut pixel);
```

## Command line tools

The `tools` workspace member (`ocio-tools`, not published) ports the OCIO
applications:

| Tool               | Purpose                                        |
|--------------------|------------------------------------------------|
| `ocioarchive`      | Create, list and extract OCIOZ archives        |
| `ociobakelut`      | Bake a color transform into a LUT              |
| `ociocheck`        | Validate a config                              |
| `ociochecklut`     | Apply a LUT file to a color                    |
| `ocioconvert`      | Convert an image between color spaces          |
| `ociolutimage`     | Convert between LUTs and LUT images            |
| `ociomakeclf`      | Create a CLF file                              |
| `ociomergeconfigs` | Merge configs                                  |
| `ocioperf`         | Benchmark a transform                          |
| `ociowrite`        | Write a config or transform to a file          |

Run one with `cargo run -p ocio-tools --release --bin ociocheck -- config.ocio`.

## Development

```sh
cargo test --all-targets
cargo test --doc
```

See [`CLAUDE.md`](CLAUDE.md) for the testing conventions.

To execute the generated WGSL on a GPU and benchmark it against the CPU
processor, see the standalone [`wgpu-example`](wgpu-example/README.md).

## License

`BSD-3-Clause`, copyright Contributors to the OpenColorIO Project; see
[`LICENSE`](LICENSE).

ocio-rs is a derivative work of
[OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO): its
code is translated from the C++ sources, and its built-in configs and test
fixtures are copied from the OpenColorIO repositories. It is therefore
distributed under the same license and keeps the upstream copyright notice.
Every source file carries the upstream SPDX header:

```rust
// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
```

ocio-rs is not affiliated with or endorsed by the OpenColorIO Project or the
Academy Software Foundation.

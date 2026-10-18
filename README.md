# ocio-rs

A pure Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
(OCIO), the color management framework for visual effects and animation.

The crate is named `ocio`. It has no C++ dependency, and its CPU renderers
reproduce the results of OCIO built with SSE2 on every target.

## Status

The port follows OCIO `main` between `v2.5.0` and `v2.6.0`, and the library
reports version `2.5.2`. See [`UPSTREAM.md`](UPSTREAM.md) for the exact
upstream revision and the changes still missing for `v2.6.0`, and
[`PORTING.md`](PORTING.md) for the architecture and the numerics notes.

What is covered:

* Configs: YAML parsing and writing, contexts and environment variables,
  file rules, viewing rules, looks, named transforms, built-in configs and
  OCIOZ archives.
* Transforms and ops: the OCIO transform set, op optimization, and a CPU
  processor working on interleaved RGBA `f32`.
* File formats: the LUT and transform formats OCIO reads and writes (CLF/CTF,
  `.cube`, `.spi1d`, `.spi3d`, `.3dl`, ...), and LUT baking.
* GPU shader text generation for GLSL, GLSL ES, Vulkan GLSL, HLSL, MSL, Cg
  and OSL. There is no GPU execution backend.

## Usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ocio = "0.2"
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

## License

`MIT AND BSD-3-Clause`; see [`LICENSE`](LICENSE).

# WGSL execution and CPU/GPU benchmark

Validate the generated WGSL on a real adapter:

```sh
cargo run --release --manifest-path wgpu-example/Cargo.toml
```

Validate and benchmark:

```sh
cargo run --release --manifest-path wgpu-example/Cargo.toml -- --bench --iterations 7
```

The example prints the selected adapter and backend. It uses the default
`cg-config-latest` builtin config to transform ACEScg to `sRGB - Display` /
`ACES 2.0 - SDR 100 nits (Rec.709)`, preceded by dynamic exposure.

Validation covers 4,363 deterministic RGBA pixels: negative values, black,
primaries, a scene-linear ramp, chromatic HDR samples and varying alpha.
Exposures -2, 0, +1.5 and +4 update the uniform buffer without rebuilding
the shader or LUTs. Results must agree within two code values after clamping
to [0, 1] and rounding to 8 bits. This is a display-output tolerance, not
bit-exact or unclamped floating-point equivalence. The program exits with
status 1 on disagreement, or 2 when adapter/device creation fails.

## Timing methodology

The benchmark repeats the validation pixels to fill 1280x720, 1920x1080 and
3840x2160 frames. It reports median wall time after two warmups per size.
Each size's first full-frame round trip is checked against the CPU outside
the timed regions. Sizes exceeding the device's buffer limits are skipped.
The compute wrapper uses a two-dimensional dispatch to support 4K within
the workgroup-count limit.

* **CPU:** single-threaded `CpuProcessor::apply_pixels`, processing RGBA f32
  in place. Resetting the input buffer is excluded from timing.
* **GPU resident:** buffers and bind groups are reused; input is already on
  the GPU. Timing includes command encoding, queue submission, compute and
  a blocking completion wait. This is host wall time, not GPU timestamp time.
* **GPU round trip:** the example's complete `Runner::run`, including buffer
  allocation, byte packing, upload, compute, readback mapping and unpacking.
  The returned output allocation's final destruction is excluded.

Shader compilation, processor creation and initial LUT upload are excluded.
The CPU uses its default optimizations. These numbers describe this complex
ACES display transform against the library's single-thread CPU path; other
transforms, images, adapters and multithreaded CPU applications will differ.
For repeatable measurements, run without concurrent builds or other GPU work.

## Measured results (2026-11-01)

Windows, Intel Core i7-5960X @ 3.00 GHz, NVIDIA GeForce GTX 980 Ti,
Vulkan backend, NVIDIA driver 560.94, Rust 1.99.0. Release build, seven
measured samples after two warmups, with no concurrent builds/tests.

| Frame | CPU ms | GPU resident ms | GPU round trip ms | Resident speedup | Round trip speedup |
|---|---:|---:|---:|---:|---:|
| 1280x720 | 691.280 | 0.700 | 24.374 | 987.54x | 28.36x |
| 1920x1080 | 1508.006 | 3.817 | 82.738 | 395.12x | 18.23x |
| 3840x2160 | 5889.333 | 2.257 | 183.197 | 2609.83x | 32.15x |

All four exposure checks had a maximum difference of one 8-bit code value
and zero non-finite GPU values. The full-frame checks passed at every size.
The nine tests in `cargo test --test gpu_wgsl --locked` also passed (shader
validation and CPU interpretation, complementary to this real-GPU run).
Formatting and release Clippy with `-D warnings` passed for the example.

Resident timings are short and sensitive to scheduling and GPU power state;
the 1080p time exceeding the 4K time illustrates that variability. Treat
these as measurements of this run, rather than a scaling guarantee.
The round trip timings show the substantial transfer/allocation cost of
the example's host-buffer workflow.

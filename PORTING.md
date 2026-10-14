# Porting notes

The upstream C++ revision this port follows, and what is missing for the
latest OCIO release, are recorded in [`UPSTREAM.md`](UPSTREAM.md).

## Architecture

| OCIO (C++)                         | ocio-rs                                   |
|------------------------------------|-------------------------------------------|
| `OpenColorTypes.h`, `ParseUtils`   | `src/types.rs`                            |
| `OpenColorTransforms.h`            | `src/transforms/mod.rs` (plain structs)   |
| `Transform.cpp` `BuildOps`         | `src/transforms/build.rs` (`BuildOps`)    |
| `Op`, `OpData`, `*OpCPU`           | `src/ops/*` (`Op` trait, f32 RGBA)        |
| `OpOptimizers.cpp`                 | `src/processor.rs` (`optimize_ops`)       |
| `Processor`, `CPUProcessor`        | `src/processor.rs`                        |
| `ImageDesc`                        | `src/image_desc.rs`                       |
| `Context`, `ContextVariableUtils`  | `src/context.rs`                          |
| `Config`, `OCIOYaml`, rules, ...   | `src/config/*`                            |
| `fileformats/*`, `FileTransform`   | `src/fileformats/*`                       |
| `transforms/builtins`, `builtinconfigs` | `src/builtins/*`                     |
| `Baker`, `BakingUtils`             | `src/baker.rs`                            |

Design choices:

* Transforms are plain structs with public fields, wrapped in the `Transform` enum.
* Each transform implements `BuildOps` (transform → ops) and `Validate`.
* Ops implement the `Op` trait and always process interleaved RGBA `f32`.
  Optimization is driven by `Op::combine_with` (pair composition / cancellation)
  and `Op::is_identity`.
* File formats implement `FileFormat` and produce a `GroupTransform`, so no format
  has format-specific op building.
* GPU shader generation is ported (`src/gpu`, `src/ops/*/gpu.rs`): shader text
  for every `GpuLanguage` (GLSL, GLSL ES, Vulkan GLSL, HLSL, MSL, Cg, OSL);
  no GPU execution backend. As in OCIO, OSL has no texture support, so 1D / 3D
  LUT ops are rejected: a processor containing one returns an error when OSL
  shader text is requested.

## CPU numerics: OCIO's SSE2 builds

The CPU renderers reproduce the results of OCIO built with SSE2
(`OCIO_USE_SSE2`), which is how OCIO is shipped (the PyOpenColorIO wheels,
DCC applications such as Nuke): where the SSE2 code computes something
differently from the scalar fallback, ocio-rs follows the SSE2 code. The
SSE2 instructions are emulated lane by lane in portable Rust
(`src/ops/sse.rs`, a port of `SSE.h`), so the results are the same on every
target, ARM included.

* **`OptimizationFlags::FAST_LOG_EXP_POW`** (part of `DEFAULT`, `VERY_GOOD`,
  `GOOD` and `DRAFT`) is honoured as in `CPUProcessor::Impl::finalize`: the
  CPU processor asks each op for its CPU renderer (`Op::cpu_op`, port of
  `Op::getCPUOp`), and the gamma, log, CDL and PQ (`LIN_TO_PQ`,
  `PQ_TO_LIN`) ops then use ports of their `*SSE` renderers, built on the
  polynomial `sseLog2` / `sseExp2` / `ssePower` (about 15 bits of mantissa).
  Their lane behaviour is kept: the gamma power also processes alpha
  (`1^1` gives `1.0000116`), the SSE camera log curves pick the log segment
  for `x > break`, and the CDL luma sum includes `alpha * 0`.
* **Renderers without a flag** that SSE2 builds always evaluate with SSE2
  use that path: ExposureContrast (linear and video styles), GradingPrimary
  (all styles: fast powers, `_mm_min_ps` / `_mm_max_ps` clamps, alpha lane
  in the luma sum), the lin-to-log / log-to-lin shapers of GradingRGBCurve
  and GradingTone (which, as in OCIO's in-place processing, also transform
  alpha), and the matrix renderer (`(r*m0 + g*m1) + (b*m2 + a*m3)`).
* **NaN signs.** Where the sign of a NaN can become a signed value later
  (e.g. the fast mirrored gamma turns `-NaN` into `-0`), the matrix renderer
  recomputes NaN results with the x86 NaN rules and the operand order of the
  PyOpenColorIO 2.6.0 Linux x86-64 wheel. Other builds may order the
  operands differently.

Not reproduced: the MSVC + AVX builds evaluate the non-fast PQ curves with
the SVML `_mm_pow_ps`; ocio-rs uses the scalar `powf` path there.

`GetLogSideBreak` (camera log curves) resolves `log2(float)` to the double
precision C function with libstdc++ (Linux) and to the float overload with
MSVC, which moves the break point by one ulp for S-Log3: ocio-rs follows the
OCIO build of the target platform (double on Linux, float elsewhere).

These choices were validated against PyOpenColorIO 2.6.0 (Linux x86-64): the
processors of the eight built-in configs (every display / view and every
color space from `ACES2065-1`) and the gamma, log, CDL, PQ, exposure /
contrast and grading ops in all their styles and directions give bit
identical results under the `DEFAULT`, `LOSSLESS` and `NONE` flags, on
probe images including negative, denormal, huge, infinite and NaN values
(NaNs compared as NaNs). `tests/sse_parity.rs` keeps a sample of these
values.

# Porting notes

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
* GPU shader generation is not ported (CPU only).

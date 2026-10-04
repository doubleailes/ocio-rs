# ocio-rs

A pure Rust port of [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO)
(OCIO), the color management framework for visual effects and animation.

Work in progress. See `PORTING.md` for the architecture and the status of each module.

## Cargo features

Loading a config and converting colours on the CPU is always built: the config
reader and writer, the builtin configs (`ocio://…`) and builtin transforms, every
CPU op, processors and their optimisation, displays, views, looks, named
transforms, file and viewing rules. Everything else is a feature, all of them
on by default:

| feature | adds | without it |
|---------|------|------------|
| `file-formats` | the LUT and CDL file readers and writers (`ocio::fileformats`): `FileTransform`, `CdlTransform::create_from_file`, and baking LUTs (`ocio::Baker`) | a config using a `FileTransform` still loads; building a processor that needs one is an error naming the feature (a missing file is still reported as missing), and `FileTransform` reports no readable format |
| `ocioz` | `.ocioz` config archives: reading (`Config::create_from_file`, `Config::create_from_archive`) and writing (`Config::archive`); the `zip` dependency | opening an archive is an error naming the feature. An archive written without `file-formats` leaves out the LUT files |
| `gpu` | GPU shader generation (`ocio::gpu`, `Processor::*_gpu_processor`) | `ocio::gpu` and the GPU processor methods do not exist |
| `apphelpers` | the application helpers (`ocio::apphelpers`): menus, config merging, the legacy viewing pipeline, mixing | `ocio::apphelpers` does not exist |

A host that converts colours on the CPU through configs made of builtin
transforms, such as the builtin ACES CG and studio configs, needs none of them:

```toml
ocio = { version = "0.1", default-features = false }
```

One that also reads studio configs with LUTs, archived or not:

```toml
ocio = { version = "0.1", default-features = false, features = ["file-formats", "ocioz"] }
```

Applications that draw on the GPU or build colour menus keep the defaults:

```toml
ocio = "0.1"
```

With no features, a clean release build of the library takes 24 s instead of
39 s (the `ocio` crate alone: 13 s instead of 25 s), pulls in 13 fewer
crates, and its `.rlib` is 10.9 MB instead of 18.9 MB (4 cores, Rust 1.97).

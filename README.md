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
| `file-formats` | the LUT and CDL file readers and writers (`ocio::fileformats`): `FileTransform`, `CdlTransform::create_from_file`, and baking LUTs (`ocio::Baker`) | a config using a `FileTransform` still loads; building a processor that needs one is an error (a missing file is still reported as missing) |
| `ocioz` | `.ocioz` config archives: reading (`Config::create_from_file`) and writing (`Config::archive`); the `zip` dependency | opening an archive is an error |
| `gpu` | GPU shader generation (`ocio::gpu`, `Processor::*_gpu_processor`) | — |
| `apphelpers` | the application helpers (`ocio::apphelpers`): menus, config merging, the legacy viewing pipeline, mixing | — |

A host that only converts colours through configs made of builtin transforms,
such as the ACES CG and studio configs, needs none of them:

```toml
ocio = { version = "0.1", default-features = false }
```

and one that also reads studio configs with LUTs adds `file-formats` (and
`ocioz` for archived configs). The library alone builds in 13 s with no
features against 33 s with all of them (release, one crate).

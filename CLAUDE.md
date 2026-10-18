# ocio-rs

See `PORTING.md` for the architecture and `UPSTREAM.md` for the upstream
revision the port follows.

## Testing conventions

| Test type         | Location                                   | Purpose                                              | API visibility      |
|-------------------|--------------------------------------------|------------------------------------------------------|---------------------|
| Unit tests        | `src/**/*.rs`, in a `#[cfg(test)] mod tests` | Isolated logic, edge cases, private functions        | Private and public  |
| Integration tests | `tests/*.rs`                               | End-to-end behavior through the public interface     | Public only         |
| Doc tests         | `///` / `//!` comments in `src/**/*.rs`    | Runnable examples in the generated documentation     | Public only         |

* Put unit tests at the bottom of the file they test (or in a sibling
  `tests.rs` declared with `#[cfg(test)] mod tests;` when large), so they
  can reach private items and stay next to the code. Always gate them with
  `#[cfg(test)]` so test code and `[dev-dependencies]` never reach normal
  builds.
* Integration tests in `tests/` use only the `pub` API, like a downstream
  consumer. Shared helpers go in a subdirectory module (e.g.
  `tests/config_common/mod.rs`), not in a top-level `tests/*.rs` file.
* Test fixtures live in `tests/data/` and are located through
  `env!("CARGO_MANIFEST_DIR")`, never through paths outside the package.
* Tests ship with the published crate so distribution packagers and users
  can run `cargo test` from the `.crate` source. Do not exclude `tests/` or
  unit tests from the package. crates.io caps a crate at 10 MB compressed:
  when adding large fixtures, check `cargo package --list` / the packaged
  size, and only list a fixture under `[package] exclude` if the tests that
  use it are skipped when it is missing.
* Run `cargo test --all-targets` and `cargo test --doc` before pushing
  (`--all-targets` does not run doc tests).

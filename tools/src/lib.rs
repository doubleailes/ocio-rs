//! # ocio-tools
//!
//! Shared code of the command line tools of the `ocio` crate, ports of the
//! OpenColorIO applications (`ociocheck`, `ociochecklut`, `ociobakelut`,
//! `ociowrite`, `ociomakeclf`, `ocioarchive`, `ocioconvert`, `ociolutimage`
//! and `ocioperf`):
//!
//! * [`argparse`]: the command line parser of the OCIO apps,
//! * [`imageio`]: image reading / writing (OpenEXR, PNG, TIFF, JPEG),
//! * [`viewing`]: the legacy viewing pipeline and display / view helpers,
//! * a few utilities shared by the binaries (below).

pub mod argparse;
pub mod imageio;
pub mod viewing;

use ocio::config::utils::format_g;
use std::time::{Duration, Instant};

/// The library version (`OCIO::GetVersion`).
pub fn version() -> &'static str {
    ocio::OCIO_VERSION
}

/// The library version as an integer, `0xMMmmpp00` (`OCIO::GetVersionHex`).
pub fn version_hex() -> u32 {
    let mut parts = ocio::OCIO_VERSION
        .split('.')
        .map(|p| p.trim().parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major << 24) | (minor << 16) | (patch << 8)
}

/// Value of an environment variable, `None` when unset
/// (`OCIO::GetEnvVariable`).
pub fn env_variable(name: &str) -> Option<String> {
    std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
}

/// The command line arguments (lossy UTF-8 conversion).
pub fn command_line_args() -> Vec<String> {
    std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

/// Format a float like a C++ stream with the default float notation and
/// the given precision (`%.<precision>g`).
pub fn fmt_float(v: f64, precision: usize) -> String {
    format_g(v, precision)
}

/// Format a float like a C++ stream with the default settings
/// (precision 6).
pub fn fmt_default(v: f64) -> String {
    format_g(v, 6)
}

/// Clear the global caches of the library (`OCIO::ClearAllCaches`).
pub fn clear_all_caches() {
    ocio::fileformats::file_transform::clear_file_transform_caches();
}

/// Duration in milliseconds, as a float (`std::chrono::duration<float,
/// std::milli>`).
pub fn millis(d: Duration) -> f32 {
    (d.as_secs_f64() * 1000.0) as f32
}

/// Accumulates a processing time (port of `apputils/measure.h`). The
/// result is printed when the measure is dropped.
#[derive(Debug)]
pub struct Measure {
    explanations: String,
    iterations: u32,
    started: Option<Instant>,
    duration: f32,
}

impl Measure {
    /// A stopped measure for one iteration.
    pub fn new(explanations: &str) -> Self {
        Self::with_iterations(explanations, 1)
    }

    /// A stopped measure averaging over `iterations`.
    pub fn with_iterations(explanations: &str, iterations: u32) -> Self {
        Measure {
            explanations: explanations.to_string(),
            iterations,
            started: None,
            duration: 0.0,
        }
    }

    /// Start (or restart) the measure.
    pub fn resume(&mut self) -> ocio::Result<()> {
        if self.started.is_some() {
            return Err(ocio::Error::msg("Measure already started."));
        }
        self.started = Some(Instant::now());
        Ok(())
    }

    /// Pause the measure.
    pub fn pause(&mut self) -> ocio::Result<()> {
        match self.started.take() {
            Some(start) => {
                self.duration += millis(start.elapsed());
                Ok(())
            }
            None => Err(ocio::Error::msg("Measure already stopped.")),
        }
    }

    /// The report printed on drop.
    pub fn report(&self) -> String {
        format!(
            "\n{}\n  Processing took: {} ms",
            self.explanations,
            fmt_default(f64::from(self.duration / self.iterations.max(1) as f32))
        )
    }
}

impl Drop for Measure {
    fn drop(&mut self) {
        if self.started.is_some() {
            let _ = self.pause();
        }
        println!("{}", self.report());
    }
}

/// Measure collecting the duration of every iteration (port of the
/// `CustomMeasure` class of `ocioperf`). The first, average of the others
/// and overall average durations are printed when dropped.
#[derive(Debug)]
pub struct CustomMeasure {
    explanations: String,
    iterations: u32,
    started: Option<Instant>,
    duration: f32,
    durations: Vec<f32>,
}

impl CustomMeasure {
    /// A stopped measure for `iterations`.
    pub fn new(explanations: &str, iterations: u32) -> Self {
        CustomMeasure {
            explanations: explanations.to_string(),
            iterations,
            started: None,
            duration: 0.0,
            durations: Vec::new(),
        }
    }

    /// Start one iteration.
    pub fn resume(&mut self) -> ocio::Result<()> {
        if self.started.is_some() {
            return Err(ocio::Error::msg("Measure already started."));
        }
        self.started = Some(Instant::now());
        Ok(())
    }

    /// Stop one iteration.
    pub fn pause(&mut self) -> ocio::Result<()> {
        match self.started.take() {
            Some(start) => {
                let d = millis(start.elapsed());
                self.durations.push(d);
                self.duration += d;
                Ok(())
            }
            None => Err(ocio::Error::msg("Measure already stopped.")),
        }
    }

    /// The report printed on drop (empty for zero iterations).
    pub fn report(&self) -> String {
        if self.iterations == 0 {
            return String::new();
        }
        let first = self.durations.first().copied().unwrap_or(0.0);
        let mut s = format!(
            "{:>9}For {} iterations, it took: [{}",
            // The C++ sets a width of 9 on the stream, which only pads the
            // first inserted string.
            self.explanations,
            self.iterations,
            fmt_float(f64::from(first), 6)
        );
        if self.iterations > 1 {
            let others = (self.duration - first) / (self.iterations - 1) as f32;
            let all = self.duration / self.iterations as f32;
            s.push_str(&format!(
                ", {}, {}",
                fmt_float(f64::from(others), 6),
                fmt_float(f64::from(all), 6)
            ));
        }
        s.push_str("] ms");
        s
    }
}

impl Drop for CustomMeasure {
    fn drop(&mut self) {
        if self.started.is_some() {
            let _ = self.pause();
        }
        if self.iterations > 0 {
            println!("{}", self.report());
        }
    }
}

/// Remove the extension of the last component of `path` (port of the
/// minizip `mz_path_remove_extension`).
pub fn remove_extension(path: &str) -> String {
    for (i, c) in path.char_indices().rev() {
        if c == '/' || c == '\\' {
            break;
        }
        if c == '.' {
            return path[..i].to_string();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(version_hex(), 0x0205_0000);
        assert_eq!(remove_extension("dir/archive.ocioz"), "dir/archive");
        assert_eq!(remove_extension("dir.x/archive"), "dir.x/archive");
        assert_eq!(remove_extension("a.b.c"), "a.b");
        assert_eq!(fmt_float(0.18, 7), "0.18");
        assert_eq!(fmt_default(1.0 / 3.0), "0.333333");
    }

    #[test]
    fn custom_measure_report() {
        let mut m = CustomMeasure::new("Test:\t", 2);
        m.durations = vec![2.0, 1.0];
        m.duration = 3.0;
        assert_eq!(
            m.report(),
            "   Test:\tFor 2 iterations, it took: [2, 1, 1.5] ms"
        );
        m.iterations = 0;
    }
}

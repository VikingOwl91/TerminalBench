//! Timing backend — replaces both lib/stats.sh's zsh-timer fallback and
//! the external hyperfine dependency. `std::time::Instant` is a monotonic
//! clock read with no fork per sample, and warmup + IQR filtering matches
//! what hyperfine provided, so a single built-in backend ("rust-timer")
//! covers both roles. Backend ids are registered in Reporting Spec v1's
//! profile registry (docs/reporting-spec-v1.md — registry amendment).

use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

use crate::stats;

pub const BACKEND_TIMER: &str = "rust-timer";

/// Adaptive-sampling tunables (--runs auto), unchanged from lib/stats.sh.
pub const ADAPTIVE_BATCH: usize = 10;
pub const ADAPTIVE_CAP: usize = 50;
pub const ADAPTIVE_TARGET_PCT: f64 = 5.0;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Runs {
    Fixed(usize),
    Auto,
}

impl Runs {
    pub fn parse(s: &str) -> Option<Runs> {
        if s == "auto" {
            return Some(Runs::Auto);
        }
        let n: usize = s.parse().ok()?;
        if n >= 1 {
            Some(Runs::Fixed(n))
        } else {
            None
        }
    }
}

/// Where a measured command's stdout goes. `Terminal` is the
/// render/scroll path: bytes must genuinely reach the emulator's render
/// pipeline via the controlling tty. Without one the scenario is not
/// measurable and must be skipped; silently changing the sink would change
/// the registered workload while retaining its profile identity.
#[derive(Clone, Copy, PartialEq)]
pub enum Sink {
    Null,
    Terminal,
}

fn open_sink(sink: Sink) -> std::io::Result<Stdio> {
    match sink {
        Sink::Null => Ok(Stdio::null()),
        Sink::Terminal => File::options()
            .write(true)
            .open("/dev/tty")
            .map(Stdio::from)
            .map_err(|e| Error::new(e.kind(), format!("controlling tty unavailable: {e}"))),
    }
}

/// Measured commands are argv vectors — no shell in between, so the
/// timed boundary is exactly the process being measured. (The zsh
/// implementation used `eval` on a command string; scenario commands are
/// simple enough that argv splitting on whitespace preserves behavior.)
pub fn split_command(cmd: &str) -> Vec<String> {
    // Minimal quote-aware splitter for the fixed scenario command shapes
    // ("{shell} -i -c exit", "cat '/path/with spaces'").
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                c if c.is_whitespace() => {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(c),
            },
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn run_once(argv: &[String], sink: Sink) -> std::io::Result<f64> {
    run_once_with_timeout(argv, sink, COMMAND_TIMEOUT)
}

fn run_once_with_timeout(argv: &[String], sink: Sink, timeout: Duration) -> std::io::Result<f64> {
    if argv.is_empty() {
        return Err(Error::new(ErrorKind::InvalidInput, "empty command"));
    }
    let stdout = open_sink(sink)?;
    let start = Instant::now();
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()?;
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill()?;
            let _ = child.wait()?;
            return Err(Error::new(
                ErrorKind::TimedOut,
                format!(
                    "measured command exceeded {:.0}s timeout",
                    timeout.as_secs_f64()
                ),
            ));
        }
    };
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    if !status.success() {
        return Err(Error::other(format!(
            "measured command exited unsuccessfully: {status}"
        )));
    }
    Ok(ms)
}

pub struct Collected {
    pub backend: &'static str,
    pub outliers_removed: usize,
    /// Trials kept by the outlier filter, in trial order.
    pub samples_ms: Vec<f64>,
    /// Every timed trial, in trial order. p95 is computed from these.
    pub raw_ms: Vec<f64>,
}

/// Fixed-count collection: warmup trials discarded, then `runs` timed
/// trials, then 1.5×IQR filtering.
pub fn collect_fixed(
    argv: &[String],
    runs: usize,
    warmup: usize,
    sink: Sink,
) -> std::io::Result<Collected> {
    for _ in 0..warmup {
        let _ = run_once(argv, sink)?;
    }
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        samples.push(run_once(argv, sink)?);
    }
    let (removed, kept) = stats::iqr_filter(&samples);
    Ok(Collected {
        backend: BACKEND_TIMER,
        outliers_removed: removed,
        samples_ms: kept,
        raw_ms: samples,
    })
}

/// `--runs auto`: collect raw samples in batches, apply one global IQR filter,
/// and stop once the bootstrap 95% CI half-width on the retained median is
/// within ADAPTIVE_TARGET_PCT, or after ADAPTIVE_CAP raw trials.
pub fn collect_adaptive(argv: &[String], warmup: usize, sink: Sink) -> std::io::Result<Collected> {
    for _ in 0..warmup {
        let _ = run_once(argv, sink)?;
    }

    let mut raw = Vec::with_capacity(ADAPTIVE_CAP);
    while raw.len() < ADAPTIVE_CAP {
        let batch = ADAPTIVE_BATCH.min(ADAPTIVE_CAP - raw.len());
        for _ in 0..batch {
            raw.push(run_once(argv, sink)?);
        }
        let (_, kept) = stats::iqr_filter(&raw);
        if kept.len() >= ADAPTIVE_BATCH
            && stats::bootstrap_ci_halfwidth_pct(&kept) <= ADAPTIVE_TARGET_PCT
        {
            break;
        }
    }
    let (outliers_removed, samples_ms) = stats::iqr_filter(&raw);
    Ok(Collected {
        backend: BACKEND_TIMER,
        outliers_removed,
        samples_ms,
        raw_ms: raw,
    })
}

pub fn collect(
    argv: &[String],
    runs: Runs,
    warmup: usize,
    sink: Sink,
) -> std::io::Result<Collected> {
    match runs {
        Runs::Fixed(n) => collect_fixed(argv, n, warmup, sink),
        Runs::Auto => collect_adaptive(argv, warmup, sink),
    }
}

/// Background output flood for "loaded"/"load" variants: streams the
/// fixture to the tty in a loop from a child
/// process group, so `stop` can kill the loop and its current `cat`-
/// equivalent together. The child is this same binary in a hidden
/// `__flood` mode — no shell involved.
pub struct Flood {
    child: std::process::Child,
}

pub fn flood_start(fixture: &Path) -> std::io::Result<Flood> {
    let meta = fixture.metadata()?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "flood fixture is empty or not a file: {}",
                fixture.display()
            ),
        ));
    }
    File::options()
        .write(true)
        .open("/dev/tty")
        .map_err(|e| Error::new(e.kind(), format!("controlling tty unavailable: {e}")))?;
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe)
        .arg("__flood")
        .arg(fixture)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Let the flood ramp up before the caller starts sampling.
    std::thread::sleep(std::time::Duration::from_millis(200));
    if let Some(status) = child.try_wait()? {
        return Err(Error::other(format!(
            "output flood exited before sampling: {status}"
        )));
    }
    Ok(Flood { child })
}

impl Flood {
    /// Stop a flood that was still alive at the end of sampling. If it
    /// exited on its own, the caller must discard the loaded variant because
    /// the offered load was not present for the full measurement window.
    pub fn stop(mut self) -> std::io::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            return Err(Error::other(format!(
                "output flood exited during sampling: {status}"
            )));
        }
        self.child.kill()?;
        let _ = self.child.wait()?;
        Ok(())
    }
}

/// `tbench __flood FIXTURE` — internal: loop-write the fixture to the tty
/// until killed. Exits unsuccessfully if setup or output fails.
pub fn flood_main(fixture: &Path) -> ! {
    let data = match std::fs::read(fixture) {
        Ok(data) if !data.is_empty() => data,
        _ => std::process::exit(1),
    };
    let mut target = match File::options().write(true).open("/dev/tty") {
        Ok(f) => f,
        Err(_) => std::process::exit(1),
    };
    loop {
        if target.write_all(&data).is_err() {
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_commands() {
        assert_eq!(
            split_command("cat '/tmp/a b.txt'"),
            vec!["cat".to_string(), "/tmp/a b.txt".to_string()]
        );
        assert_eq!(
            split_command("/bin/zsh -i -c exit"),
            vec!["/bin/zsh", "-i", "-c", "exit"]
        );
    }

    #[test]
    fn runs_parse() {
        assert_eq!(Runs::parse("auto"), Some(Runs::Auto));
        assert_eq!(Runs::parse("20"), Some(Runs::Fixed(20)));
        assert_eq!(Runs::parse("0"), None);
        assert_eq!(Runs::parse("x"), None);
    }

    #[test]
    fn unsuccessful_command_is_not_a_sample() {
        let argv = vec!["/usr/bin/false".to_string()];
        assert!(collect_fixed(&argv, 1, 0, Sink::Null).is_err());
    }

    #[test]
    fn hanging_command_is_stopped_and_not_sampled() {
        let argv = vec!["/bin/sleep".to_string(), "1".to_string()];
        let start = Instant::now();
        assert!(run_once_with_timeout(&argv, Sink::Null, Duration::from_millis(20)).is_err());
        assert!(start.elapsed() < Duration::from_millis(500));
    }
}

//! Scenario dispatch — port of lib/runner.sh. Loads each scenarios/*.yaml,
//! dispatches on `kind`, and appends ResultRows. Settle pauses between
//! scenarios/variants (proposal §A item 1) are the caller's/our shared
//! guard against cross-contamination.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use crate::measure::{self, Runs, Sink};
use crate::report::ResultRow;
use crate::scenario::Scenario;
use crate::stats;

pub struct RunConfig {
    pub root: PathBuf,
    pub shells: Vec<String>,
    pub runs: Runs,
    pub warmup: usize,
    pub settle_secs: f64,
    pub keep_samples: bool,
}

pub fn settle(cfg: &RunConfig) {
    if cfg.settle_secs > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(cfg.settle_secs));
    }
}

pub fn run_scenario(scenario: &Scenario, cfg: &RunConfig, results: &mut Vec<ResultRow>) -> bool {
    match scenario.kind() {
        "shell" => run_shell(scenario, cfg, results),
        "render_throughput" => run_render_throughput(scenario, cfg, results),
        "scroll_proxy" => run_scroll_proxy(scenario, cfg, results),
        "resource_footprint" => crate::resource::run(scenario, cfg, results),
        "fire" => run_fire(scenario, cfg, results),
        other => {
            eprintln!(
                "tbench: scenario '{}' has unknown kind '{other}'",
                scenario.path.display()
            );
            false
        }
    }
}

// --- kind: shell ---------------------------------------------------------

pub fn resolve_shell_bin(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn run_shell(scenario: &Scenario, cfg: &RunConfig, results: &mut Vec<ResultRow>) -> bool {
    let (Some(metric), Some(unit), Some(cmd_template)) = (
        scenario.field("metric"),
        scenario.field("unit"),
        scenario.field("command"),
    ) else {
        eprintln!(
            "tbench: scenario '{}' is missing metric/unit/command",
            scenario.path.display()
        );
        return false;
    };
    let (metric, unit, cmd_template) = (
        metric.to_string(),
        unit.to_string(),
        cmd_template.to_string(),
    );

    let mut produced = false;
    for shell in &cfg.shells {
        let Some(shell_bin) = resolve_shell_bin(shell) else {
            eprintln!("tbench: shell '{shell}' not found on PATH, skipping");
            continue;
        };
        let cmd = cmd_template.replace("{shell}", &shell_bin.to_string_lossy());
        let argv = measure::split_command(&cmd);
        let c = match measure::collect(&argv, cfg.runs, cfg.warmup, Sink::Null) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("tbench: failed to run '{cmd}': {e}; skipping {metric} ({shell})");
                continue;
            }
        };
        // ms -> scenario unit (only "us" needs scaling today).
        let scale = if unit == "us" { 1000.0 } else { 1.0 };
        let scaled: Vec<f64> = c.samples_ms.iter().map(|v| v * scale).collect();
        results.push(ResultRow {
            metric: metric.clone(),
            shell: Some(shell.clone()),
            unit: unit.clone(),
            median: stats::median(&scaled),
            p95: stats::p95(&scaled),
            stddev: stats::stddev(&scaled),
            runs: scaled.len(),
            outliers_removed: c.outliers_removed,
            backend: c.backend.to_string(),
            variant: None,
            notes: None,
            samples: cfg.keep_samples.then(|| scaled.clone()),
        });
        produced = true;
    }
    produced
}

// --- fixtures ------------------------------------------------------------

const FIXTURE_LINES: usize = 100_000;
const LINE_TEXT: &str = "the quick brown fox jumps over the lazy dog 0123456789 abcdefghijklmnopqrstuvwxyz tbench-render-throughput-fixture-line";
const LINE_CJK: &str = "こんにちは、世界🌏 The quick brown 狐 jumps over 怠け者の犬 ASCII0123456789 絵文字✨🚀 全角テストtbench-cjk-fixture-line";

fn fixture_is_valid(path: &Path, line: &str, wrap_ansi: bool) -> std::io::Result<bool> {
    let mut expected = Vec::new();
    if wrap_ansi {
        expected.extend_from_slice(b"\x1b[32m");
    }
    expected.extend_from_slice(line.as_bytes());
    if wrap_ansi {
        expected.extend_from_slice(b"\x1b[0m");
    }
    expected.push(b'\n');

    let meta = path.metadata()?;
    if !meta.is_file() || meta.len() != (expected.len() * FIXTURE_LINES) as u64 {
        return Ok(false);
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut actual = Vec::with_capacity(expected.len());
    for _ in 0..FIXTURE_LINES {
        actual.clear();
        reader.read_until(b'\n', &mut actual)?;
        if actual != expected {
            return Ok(false);
        }
    }
    let mut trailing = [0u8; 1];
    Ok(reader.read(&mut trailing)? == 0)
}

/// Byte-identical port of fixtures/gen-fixtures.sh: validates and lazily
/// generates the three large-output fixtures on first use. A truncated or
/// modified fixture is rebuilt instead of being accepted merely as nonempty.
pub fn gen_fixtures(root: &Path) -> bool {
    let dir = root.join("fixtures");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("tbench: could not create fixture directory: {e}");
        return false;
    }
    let write = |path: &Path, line: &str, wrap_ansi: bool| -> std::io::Result<()> {
        match fixture_is_valid(path, line, wrap_ansi) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut out = std::io::BufWriter::new(File::create(path)?);
        for _ in 0..FIXTURE_LINES {
            if wrap_ansi {
                out.write_all(b"\x1b[32m")?;
            }
            out.write_all(line.as_bytes())?;
            if wrap_ansi {
                out.write_all(b"\x1b[0m")?;
            }
            out.write_all(b"\n")?;
        }
        out.flush()?;
        if fixture_is_valid(path, line, wrap_ansi)? {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "generated fixture failed validation: {}",
                path.display()
            )))
        }
    };
    let ok = write(&dir.join("large-output-plain.txt"), LINE_TEXT, false).is_ok()
        && write(&dir.join("large-output-ansi.txt"), LINE_TEXT, true).is_ok()
        && write(&dir.join("large-output-cjk.txt"), LINE_CJK, false).is_ok();
    if !ok {
        eprintln!("tbench: fixture generation failed");
    }
    ok
}

// --- kind: render_throughput ---------------------------------------------

fn run_render_throughput(
    scenario: &Scenario,
    cfg: &RunConfig,
    results: &mut Vec<ResultRow>,
) -> bool {
    let metric = scenario
        .field("metric")
        .unwrap_or("render.throughput")
        .to_string();
    let notes = scenario.field("notes").map(str::to_string);
    if !gen_fixtures(&cfg.root) {
        eprintln!("tbench: fixture generation failed, skipping render.throughput");
        return false;
    }

    let mut produced = false;
    for (i, variant) in ["plain", "ansi", "cjk"].iter().enumerate() {
        if i > 0 {
            settle(cfg);
        }
        let fixture = cfg
            .root
            .join(format!("fixtures/large-output-{variant}.txt"));
        let Ok(meta) = fixture.metadata() else {
            eprintln!(
                "tbench: fixture {} missing, skipping render.throughput/{variant}",
                fixture.display()
            );
            continue;
        };
        let size_bytes = meta.len() as f64;
        let line_count = FIXTURE_LINES as f64;

        let argv = vec!["cat".to_string(), fixture.to_string_lossy().into_owned()];
        let c = match measure::collect(&argv, cfg.runs, cfg.warmup, Sink::Terminal) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("tbench: render.throughput/{variant} failed: {e}; skipping");
                continue;
            }
        };
        let mut mbps = Vec::new();
        let mut linesps = Vec::new();
        for t in &c.samples_ms {
            if *t <= 0.0 {
                continue;
            }
            mbps.push((size_bytes / 1_000_000.0) / (t / 1000.0));
            linesps.push(line_count / (t / 1000.0));
        }
        if mbps.is_empty() {
            eprintln!("tbench: render.throughput/{variant} produced no usable samples");
            continue;
        }
        for (unit, samples) in [("MBps", &mbps), ("linesps", &linesps)] {
            results.push(ResultRow {
                metric: metric.clone(),
                shell: None,
                unit: unit.to_string(),
                median: stats::median(samples),
                p95: stats::p95(samples),
                stddev: stats::stddev(samples),
                runs: c.samples_ms.len(),
                outliers_removed: c.outliers_removed,
                backend: c.backend.to_string(),
                variant: Some(variant.to_string()),
                notes: notes.clone(),
                samples: cfg.keep_samples.then(|| samples.clone()),
            });
        }
        produced = true;
    }
    produced
}

// --- kind: scroll_proxy --------------------------------------------------

fn run_scroll_proxy(scenario: &Scenario, cfg: &RunConfig, results: &mut Vec<ResultRow>) -> bool {
    let metric = scenario
        .field("metric")
        .unwrap_or("scroll.proxy")
        .to_string();
    let unit = scenario.field("unit").unwrap_or("ms").to_string();
    let notes = scenario.field("notes").map(str::to_string);
    if !gen_fixtures(&cfg.root) {
        eprintln!("tbench: fixture generation failed, skipping scroll.proxy");
        return false;
    }
    let fixture = cfg.root.join("fixtures/large-output-plain.txt");
    if !fixture.exists() {
        eprintln!(
            "tbench: fixture {} missing, skipping scroll.proxy",
            fixture.display()
        );
        return false;
    }
    let argv = vec!["cat".to_string(), fixture.to_string_lossy().into_owned()];
    let c = match measure::collect(&argv, cfg.runs, cfg.warmup, Sink::Terminal) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tbench: scroll.proxy failed: {e}; skipping");
            return false;
        }
    };
    if c.samples_ms.is_empty() {
        eprintln!("tbench: scroll.proxy produced no usable samples");
        return false;
    }
    results.push(ResultRow {
        metric,
        shell: None,
        unit,
        median: stats::median(&c.samples_ms),
        p95: stats::p95(&c.samples_ms),
        stddev: stats::stddev(&c.samples_ms),
        runs: c.samples_ms.len(),
        outliers_removed: c.outliers_removed,
        backend: c.backend.to_string(),
        variant: None,
        notes,
        samples: cfg.keep_samples.then(|| c.samples_ms.clone()),
    });
    true
}

// --- kind: fire ----------------------------------------------------------

const DEFAULT_FIRE_SECS: f64 = 5.0;
const MIN_FIRE_SECS: f64 = 0.1;
const MAX_FIRE_SECS: f64 = 60.0;

fn parse_fire_secs(value: Option<&str>) -> Result<f64, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_FIRE_SECS);
    };
    let secs: f64 = value
        .parse()
        .map_err(|_| format!("FIRE_SECS must be a number, got '{value}'"))?;
    if !secs.is_finite() || !(MIN_FIRE_SECS..=MAX_FIRE_SECS).contains(&secs) {
        return Err(format!(
            "FIRE_SECS must be finite and between {MIN_FIRE_SECS} and {MAX_FIRE_SECS} seconds"
        ));
    }
    Ok(secs)
}

fn run_fire(scenario: &Scenario, cfg: &RunConfig, results: &mut Vec<ResultRow>) -> bool {
    let metric = scenario.field("metric").unwrap_or("fire.fps").to_string();
    let unit = scenario.field("unit").unwrap_or("fps").to_string();
    let notes = scenario.field("notes").map(str::to_string);

    let mut target = match File::options().write(true).open("/dev/tty") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("tbench: fire.fps requires a controlling tty: {e}; skipping");
            return false;
        }
    };

    let runs = match cfg.runs {
        Runs::Fixed(n) => n,
        Runs::Auto => {
            eprintln!("tbench: --runs auto is not supported for fire.fps yet, using 5");
            5
        }
    };
    let n_runs = runs.clamp(1, 5);
    let fire_secs = std::env::var("FIRE_SECS").ok();
    let secs = match parse_fire_secs(fire_secs.as_deref()) {
        Ok(secs) => secs,
        Err(e) => {
            eprintln!("tbench: {e}; skipping fire.fps");
            return false;
        }
    };

    let mut fps_samples = Vec::new();
    let mut last_outcome = None;
    let mut geometry = None;
    for i in 0..n_runs {
        if i > 0 {
            settle(cfg);
        }
        match crate::fire::run(secs, &mut target, true) {
            Ok(outcome) => {
                let current = outcome.region;
                if geometry.is_some_and(|expected| expected != current) {
                    eprintln!(
                        "tbench: fire.fps geometry changed between trials; discarding the scenario"
                    );
                    return false;
                }
                geometry = Some(current);
                fps_samples.push(outcome.fps);
                last_outcome = Some(outcome);
            }
            Err(e) => {
                eprintln!("tbench: fire.fps trial failed: {e}; discarding the scenario");
                return false;
            }
        }
    }
    if fps_samples.is_empty() {
        eprintln!("tbench: fire.fps produced no usable samples");
        return false;
    }
    if let Some(o) = &last_outcome {
        eprintln!(
            "tbench: fire.fps rendered region {}x{} (winsize {}x{}) — compare only at matching geometry",
            o.region.0, o.region.1, o.winsize.0, o.winsize.1
        );
    }

    results.push(ResultRow {
        metric,
        shell: None,
        unit,
        median: stats::median(&fps_samples),
        p95: stats::p95(&fps_samples),
        stddev: stats::stddev(&fps_samples),
        runs: fps_samples.len(),
        outliers_removed: 0,
        backend: "fire-v2-overlapped".to_string(),
        variant: geometry.map(|(w, rows)| format!("{w}x{rows}")),
        notes,
        samples: cfg.keep_samples.then(|| fps_samples.clone()),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_duration_is_bounded_and_finite() {
        assert_eq!(parse_fire_secs(None), Ok(DEFAULT_FIRE_SECS));
        assert_eq!(parse_fire_secs(Some("0.1")), Ok(0.1));
        assert_eq!(parse_fire_secs(Some("60")), Ok(60.0));
        for invalid in ["0", "-1", "60.1", "NaN", "inf", "not-a-number"] {
            assert!(parse_fire_secs(Some(invalid)).is_err(), "{invalid}");
        }
    }

    #[test]
    fn incomplete_fixture_is_rejected() {
        let path = std::env::temp_dir().join(format!(
            "tbench-invalid-fixture-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::write(&path, b"not the complete fixture\n").unwrap();
        assert!(!fixture_is_valid(&path, LINE_TEXT, false).unwrap());
        let _ = std::fs::remove_file(path);
    }
}

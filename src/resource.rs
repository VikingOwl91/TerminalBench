//! resource.mem / resource.cpu — port of lib/resource.sh. Targets the
//! terminal emulator's own GUI process, found by walking the ancestry to
//! the first executable inside a macOS .app bundle. `ps` remains the
//! sampling tool, so the backend id stays "ps".

use std::path::Path;
use std::process::Command;

use crate::measure;
use crate::report::ResultRow;
use crate::runner::RunConfig;
use crate::scenario::Scenario;
use crate::stats;

fn ps_field(pid: u32, field: &str) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Walk ppid links from this process up to the first .app-bundle
/// executable; None (tmux/SSH/non-macOS) means the scenario must be
/// skipped rather than misattributing resource usage.
pub fn find_terminal_pid() -> Option<(u32, String)> {
    let mut pid = std::process::id();
    while pid > 1 {
        let comm = ps_field(pid, "comm")?;
        if comm.contains(".app/Contents/MacOS/") {
            return Some((pid, comm));
        }
        let ppid: u32 = ps_field(pid, "ppid")?.parse().ok()?;
        if ppid == 0 {
            break;
        }
        pid = ppid;
    }
    None
}

fn sample_rss_mb(pid: u32) -> Option<f64> {
    let kb: f64 = ps_field(pid, "rss")?.parse().ok()?;
    Some(kb / 1024.0)
}

fn sample_cpu_pct(pid: u32) -> Option<f64> {
    ps_field(pid, "pcpu")?.parse().ok()
}

pub fn run(scenario: &Scenario, cfg: &RunConfig, results: &mut Vec<ResultRow>) -> bool {
    let mem_metric = scenario
        .field("mem_metric")
        .unwrap_or("resource.mem")
        .to_string();
    let cpu_metric = scenario
        .field("cpu_metric")
        .unwrap_or("resource.cpu")
        .to_string();
    let unit_mem = scenario.field("unit_mem").unwrap_or("MB").to_string();
    let unit_cpu = scenario.field("unit_cpu").unwrap_or("pct").to_string();
    let notes = scenario.field("notes").map(str::to_string);

    let Some((term_pid, term_comm)) = find_terminal_pid() else {
        eprintln!("tbench: could not identify the terminal emulator process (tmux/SSH/non-macOS?), skipping resource.mem/resource.cpu");
        return false;
    };
    eprintln!("tbench: resource.mem/resource.cpu targeting pid {term_pid} ({term_comm})");

    let runs = match cfg.runs {
        measure::Runs::Fixed(n) => n,
        measure::Runs::Auto => {
            eprintln!(
                "tbench: --runs auto is not supported for resource.mem/resource.cpu yet, using 5"
            );
            5
        }
    };
    let samples_per_variant = runs.clamp(1, 5);

    // Settle before *idle* sampling specifically: macOS's `ps %cpu` is a
    // decayed running average and inherits whatever ran just before this
    // scenario (the observed 243% idle p95). Floor of 5s regardless of
    // --settle.
    let idle_settle = cfg.settle_secs.max(5.0);
    eprintln!("tbench: settling {idle_settle}s before resource.*/idle sampling");
    std::thread::sleep(std::time::Duration::from_secs_f64(idle_settle));

    for variant in ["idle", "load"] {
        let mut flood = None;
        if variant == "load" {
            let fixture = cfg.root.join("fixtures/large-output-plain.txt");
            if !crate::runner::gen_fixtures(&cfg.root) {
                eprintln!("tbench: resource.*/load requires a valid flood fixture, skipping");
                continue;
            }
            match measure::flood_start(&fixture) {
                Ok(f) => flood = Some(f),
                Err(e) => {
                    eprintln!(
                        "tbench: resource.*/load could not start output flood: {e}; skipping"
                    );
                    continue;
                }
            }
        }

        let mut mem_samples = Vec::new();
        let mut cpu_samples = Vec::new();
        for i in 0..samples_per_variant {
            if let Some(m) = sample_rss_mb(term_pid) {
                mem_samples.push(m);
            }
            if let Some(c) = sample_cpu_pct(term_pid) {
                cpu_samples.push(c);
            }
            if i + 1 < samples_per_variant {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }

        if let Some(f) = flood {
            if let Err(e) = f.stop() {
                eprintln!(
                    "tbench: resource.*/{variant} output flood failed during sampling: {e}; discarding the variant"
                );
                continue;
            }
        }

        if mem_samples.is_empty() && cpu_samples.is_empty() {
            eprintln!("tbench: resource.*/{variant} produced no usable samples (process exited?)");
            continue;
        }

        for (metric, unit, samples) in [
            (&mem_metric, &unit_mem, &mem_samples),
            (&cpu_metric, &unit_cpu, &cpu_samples),
        ] {
            if samples.is_empty() {
                continue;
            }
            results.push(ResultRow {
                metric: metric.clone(),
                shell: None,
                unit: unit.clone(),
                median: stats::median(samples),
                p95: stats::p95(samples),
                stddev: stats::stddev(samples),
                runs: samples.len(),
                outliers_removed: 0,
                backend: "ps".to_string(),
                variant: Some(variant.to_string()),
                notes: notes.clone(),
                samples: cfg.keep_samples.then(|| samples.clone()),
            });
        }
    }
    true
}

/// Used by orchestrate-launched runs to keep relative paths stable.
pub fn _abs(p: &Path) -> std::path::PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

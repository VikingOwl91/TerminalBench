//! tbench — terminal/shell micro-benchmark runner, single-binary Rust
//! implementation. Subcommand surface, flags, defaults, exit codes, and
//! output conventions match the zsh/python implementation it replaces;
//! docs/reporting-spec-v1.md remains the wire contract.

mod compare;
mod fidelity;
mod fire;
mod lock;
mod measure;
mod orchestrate;
mod report;
mod resource;
mod runner;
mod scenario;
mod show;
mod stats;
mod trend;
mod validate;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use measure::Runs;

fn usage() -> &'static str {
    r#"tbench — terminal/shell micro-benchmark runner

Usage:
  tbench run [--scenario NAME] [--shells zsh,bash] [--runs N] [--app NAME]
             [--output PATH]
  tbench list
  tbench show [FILE]
  tbench compare FILE FILE [FILE ...] [--format markdown|html] [--output PATH] [--baseline NAME]
  tbench compare BASELINE.json LATEST.json --threshold 10%
  tbench trend [FILE ...] [--format markdown|html] [--output PATH]
  tbench orchestrate --apps Ghostty,iTerm,Terminal [--scenario NAME] [--runs N]
                      [--format markdown|html] [--output-dir PATH] [--timeout N]
  tbench -h | --help

Options (run):
  --scenario NAME   Run only scenarios/NAME.yaml (default: all scenarios)
  --shells LIST     Comma-separated shell names to benchmark (default: zsh,bash);
                    ignored by scenarios that measure the terminal itself
  --runs N|auto     Trials per scenario (default: 20). "auto" adaptively adds
                    trials in batches of 10 until the bootstrap 95% CI on the
                    median is within 5% of the median, capped at 50 (only
                    for shell.startup/cmd.overhead/render.throughput/
                    scroll.proxy; other scenarios fall back to a fixed default)
  --settle N        Seconds to pause between scenarios/variants (default: 2);
                    resource.*'s idle sampling always waits at least 5s
                    regardless, to let macOS's decayed %cpu average cool down
  --warmup N        Discarded warmup trials for process-timed and PTY-echo
                    metrics (default: 3; not used by resource.* or fire.fps)
  --keep-samples    Attach each result's raw per-trial samples as a
                    "samples" array in the JSON (default: off); needed for
                    `compare`'s significance testing and bootstrap CI
  --force           Skip the other-tbench-run check/wait instead of waiting
                    up to 120s — results may be polluted by cross-run
                    interference if one really is running
  --app NAME        Record NAME as the report's terminal/app identity instead
                    of detecting it from TERM_PROGRAM; wins over the
                    TBENCH_TERMINAL_ID environment override. Must be a
                    trimmed, non-empty, single-line identifier
  --output PATH     Write JSON report to PATH (default: results/tbench-<timestamp>.json)

`run` detects another concurrent `tbench run` via results/.tbench.lock (pid
recorded there) and waits for it to finish rather than risk flood/CPU
interference. Every `run` also probes DECSET 2026 (synchronized output)
support once and records it as meta.synchronized_output_supported.

`list` and `show` are read-only: neither touches results/.tbench.lock.

Options (show):
  FILE              Report JSON to summarize (default: the newest
                    results/tbench-*.json by mtime)

Options (compare):
  --format FORMAT   "markdown" (default; prints to stdout) or "html" (a
                    self-contained chart report, always written to a file:
                    --output if given, else results/compare-<timestamp>.html)
  --output PATH     Write the report to PATH (in addition to stdout for markdown)
  --threshold PCT    Regression-detection mode: requires exactly 2 files
                     (baseline, latest); exits non-zero if any metric
                     regressed by more than PCT (e.g. "10%") AND (when both
                     files have --keep-samples data for that metric)
                     Mann-Whitney p<0.05
  --baseline NAME   HTML only: which entity the Overview chart's delta bars
                    compare against (a terminal name or an input file/path);
                    default is the first file

Options (trend):
  FILE ...          Report JSONs to plot (default: all results/tbench-*.json,
                    sorted by meta.timestamp; requires at least 2)
  --format FORMAT   "markdown" (default) or "html" (line-chart report,
                    written to --output or results/trend-<timestamp>.html)
  --output PATH     Write the report to PATH (in addition to stdout for markdown)

Options (orchestrate) — macOS only:
  --apps LIST       Comma-separated app names to run in sequence, one fresh
                    window/instance each (supported: ghostty, iterm, terminal, noa)
  --scenario NAME   Passed through to each `tbench run`
  --runs N|auto     Passed through to each `tbench run` (default: 20)
  --timeout N       Seconds to wait per app before moving on (default: 600)
  --format FORMAT   Passed through to the final `tbench compare`
  --output-dir PATH Directory for per-app JSON + the compare report
                    (default: results/orchestrate-<timestamp>/)

Metrics:
  shell-scoped:      shell.startup, cmd.overhead
  terminal-scoped:   render.throughput (plain/ansi/cjk variants),
                     scroll.proxy, input.latency (idle/loaded variants),
                     resource.mem, resource.cpu (idle/load variants),
                     fire.fps (duration-based, see scenarios/fire.yaml)
                     (approximate/proxy — see README.md)

Backend: built-in monotonic timer ("rust-timer"); input.latency uses a
native pty round trip ("rust-pty"). See docs/benchmark-metrics.md.
"#
}

/// Repo root: the directory holding scenarios/. Resolved from the binary's
/// location (target/*/tbench or an installed copy next to scenarios/),
/// falling back to the current directory.
fn root_dir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            candidates.push(d.clone());
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }
    for c in &candidates {
        if c.join("scenarios").is_dir() {
            return c.clone();
        }
    }
    PathBuf::from(".")
}

fn die(msg: &str, code: u8) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(code)
}

fn die_usage(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    eprintln!("{}", usage());
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => cmd_run(&args[1..]),
        Some("list") => cmd_list(&args[1..]),
        Some("show") => cmd_show(&args[1..]),
        Some("compare") => compare::cmd_compare(&args[1..], &root_dir()),
        Some("trend") => trend::cmd_trend(&args[1..], &root_dir()),
        Some("orchestrate") => orchestrate::cmd_orchestrate(&args[1..], &root_dir()),
        Some("validate") => validate::cmd_validate(&args[1..], &root_dir()),
        Some("__smoke-report") => {
            // Hidden test hook: emit a synthetic report covering every
            // registered profile through the real assembly path.
            let doc =
                report::build_report(&report::smoke_rows(), fidelity::probe_sync_output(), None);
            println!(
                "{}",
                serde_json::to_string_pretty(&doc).expect("report serialization")
            );
            ExitCode::SUCCESS
        }
        Some("__flood") => {
            // Internal: background output flood for loaded/load variants.
            match args.get(1) {
                Some(f) => measure::flood_main(Path::new(f)),
                None => ExitCode::from(2),
            }
        }
        Some("-h") | Some("--help") => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("{}", usage());
            ExitCode::from(2)
        }
        Some(other) => die_usage(&format!("tbench: unknown command '{other}'")),
    }
}

// --- run -----------------------------------------------------------------

fn cmd_run(args: &[String]) -> ExitCode {
    let root = root_dir();
    let mut scenario_name: Option<String> = None;
    let mut shells = "zsh,bash".to_string();
    let mut runs_raw = "20".to_string();
    let mut settle: f64 = 2.0;
    let mut warmup: usize = 3;
    let mut keep_samples = false;
    let mut force = false;
    let mut app: Option<String> = None;
    let mut output: Option<PathBuf> = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        macro_rules! next_val {
            ($name:literal) => {
                match it.next() {
                    Some(v) => v.clone(),
                    None => return die_usage(concat!("tbench: ", $name, " requires a value")),
                }
            };
        }
        match arg.as_str() {
            "--scenario" => scenario_name = Some(next_val!("--scenario")),
            "--shells" => shells = next_val!("--shells"),
            "--runs" => runs_raw = next_val!("--runs"),
            "--settle" => {
                let v = next_val!("--settle");
                match v.parse::<f64>() {
                    Ok(s) if s >= 0.0 && v.chars().all(|c| c.is_ascii_digit() || c == '.') => {
                        settle = s
                    }
                    _ => return die_usage("tbench: --settle must be a non-negative number"),
                }
            }
            "--warmup" => {
                let v = next_val!("--warmup");
                match v.parse::<usize>() {
                    Ok(w) => warmup = w,
                    Err(_) => return die_usage("tbench: --warmup must be a non-negative integer"),
                }
            }
            "--keep-samples" => keep_samples = true,
            "--force" => force = true,
            "--app" => {
                let v = next_val!("--app");
                match report::trimmed_opaque_id(&v) {
                    Some(_) => app = Some(v),
                    None => {
                        return die_usage(
                            "tbench: --app must be a trimmed non-empty single-line identifier",
                        )
                    }
                }
            }
            "--output" => output = Some(PathBuf::from(next_val!("--output"))),
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other => return die_usage(&format!("tbench: unknown option '{other}'")),
        }
    }

    let Some(runs) = Runs::parse(&runs_raw) else {
        return die_usage("tbench: --runs must be a positive integer or \"auto\"");
    };

    let lockfile = root.join("results/.tbench.lock");
    let _lock = match lock::acquire(&lockfile, force) {
        Ok(l) => l,
        Err(e) => return die(&e, 1),
    };

    let scenarios = match scenario_name {
        Some(name) => {
            let f = root.join(format!("scenarios/{name}.yaml"));
            if !f.is_file() {
                return die_usage(&format!(
                    "tbench: unknown scenario '{name}' (expected {})",
                    f.display()
                ));
            }
            match scenario::Scenario::load(&f) {
                Ok(s) => vec![s],
                Err(e) => return die(&format!("tbench: failed to load {}: {e}", f.display()), 1),
            }
        }
        None => match scenario::load_all(&root) {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => {
                return die(
                    &format!(
                        "tbench: no scenarios found in {}/scenarios/",
                        root.display()
                    ),
                    1,
                )
            }
            Err(e) => return die(&format!("tbench: failed to list scenarios: {e}"), 1),
        },
    };

    let cfg = runner::RunConfig {
        root: root.clone(),
        shells: shells
            .split(',')
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect(),
        runs,
        warmup,
        settle_secs: settle,
        keep_samples,
    };

    let mut results = Vec::new();
    for (i, sc) in scenarios.iter().enumerate() {
        // Settle between scenarios too, not just between variants within
        // one — the same cross-contamination risk applies at scenario
        // boundaries.
        if i > 0 {
            runner::settle(&cfg);
        }
        runner::run_scenario(sc, &cfg, &mut results);
    }

    if results.is_empty() {
        return die(
            "tbench: no results produced (check --shells availability)",
            1,
        );
    }

    let sync_output = fidelity::probe_sync_output();
    let doc = report::build_report(&results, sync_output, app.as_deref());
    let json = serde_json::to_string_pretty(&doc).expect("report serialization");

    let output = output.unwrap_or_else(|| {
        root.join(format!(
            "results/tbench-{}.json",
            report::filename_timestamp()
        ))
    });
    if let Some(dir) = output.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&output, format!("{json}\n")) {
        return die(
            &format!("tbench: failed to write {}: {e}", output.display()),
            1,
        );
    }
    eprintln!("tbench: wrote {}", output.display());
    println!("{json}");

    // Human-readable recap on stderr — stdout above stays the
    // machine-readable JSON contract scripts pipe.
    eprintln!();
    eprintln!("{}", show::render(&doc));
    ExitCode::SUCCESS
}

// --- list ----------------------------------------------------------------

fn cmd_list(args: &[String]) -> ExitCode {
    if let Some(arg) = args.iter().next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other => return die_usage(&format!("tbench: unknown option '{other}'")),
        }
    }
    let root = root_dir();
    let scenarios = match scenario::load_all(&root) {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return die(
                &format!(
                    "tbench: no scenarios found in {}/scenarios/",
                    root.display()
                ),
                1,
            )
        }
    };

    struct Row {
        name: String,
        metric: String,
        unit: String,
        scope: &'static str,
        description: String,
    }
    let rows: Vec<Row> = scenarios
        .iter()
        .map(|sc| {
            let mut metric = sc.field("metric").unwrap_or("").to_string();
            let mut unit = sc.field("unit").unwrap_or("").to_string();
            // resource-footprint.yaml reports two metrics together.
            if metric.is_empty() {
                if let (Some(m), Some(c)) = (sc.field("mem_metric"), sc.field("cpu_metric")) {
                    metric = format!("{m}, {c}");
                    unit = format!(
                        "{}, {}",
                        sc.field("unit_mem").unwrap_or(""),
                        sc.field("unit_cpu").unwrap_or("")
                    );
                }
            }
            // render-throughput derives MBps/linesps per variant at run time.
            if unit.is_empty() && metric == "render.throughput" {
                unit = "MBps, linesps".to_string();
            }
            let scope = match metric.as_str() {
                "shell.startup" | "cmd.overhead" => "shell",
                _ => "terminal",
            };
            Row {
                name: sc.name.clone(),
                metric,
                unit,
                scope,
                description: sc.field("description").unwrap_or("").to_string(),
            }
        })
        .collect();

    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let metric_w = rows
        .iter()
        .map(|r| r.metric.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let unit_w = rows.iter().map(|r| r.unit.len()).max().unwrap_or(4).max(4);

    println!(
        "{:<name_w$}  {:<metric_w$}  {:<unit_w$}  SCOPE",
        "NAME", "METRIC", "UNIT"
    );
    let wrap_cols = 76usize;
    for r in rows {
        println!(
            "{:<name_w$}  {:<metric_w$}  {:<unit_w$}  {}",
            r.name, r.metric, r.unit, r.scope
        );
        if !r.description.is_empty() {
            for line in wrap(&r.description, wrap_cols) {
                println!("    {line}");
            }
        }
    }
    ExitCode::SUCCESS
}

fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > cols {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

// --- show ----------------------------------------------------------------

fn newest_report(root: &Path) -> Option<PathBuf> {
    let dir = root.join("results");
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("tbench-") && n.ends_with(".json"))
        })
        .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok().map(|t| (t, p)))
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().next().map(|(_, p)| p)
}

fn cmd_show(args: &[String]) -> ExitCode {
    let mut file: Option<PathBuf> = None;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") => {
                return die_usage(&format!("tbench: unknown option '{other}'"));
            }
            path => {
                if file.is_some() {
                    return die("tbench: show takes at most one FILE argument", 2);
                }
                file = Some(PathBuf::from(path));
            }
        }
    }

    let file = match file {
        Some(f) => f,
        None => match newest_report(&root_dir()) {
            Some(f) => f,
            None => {
                return die(
                    "tbench: no results/tbench-*.json reports found (run 'tbench run' first, or pass a FILE)",
                    1,
                )
            }
        },
    };
    if !file.is_file() {
        return die(&format!("tbench: file not found: {}", file.display()), 2);
    }
    let report: serde_json::Value = match std::fs::read_to_string(&file)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            return die(
                &format!("tbench: failed to read {}: {e}", file.display()),
                2,
            )
        }
    };
    println!("{}", show::render(&report));
    ExitCode::SUCCESS
}

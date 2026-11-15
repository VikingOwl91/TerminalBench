//! `tbench compare` — port of lib/compare.py (markdown table, threshold
//! regression gate, self-contained HTML chart report). Absorbs the CLI
//! wrapper behavior that lived in bin/tbench's tbench_cmd_compare (arg
//! validation, default html output path, --output file writing) so the
//! subcommand is self-contained. compare stays read-only: no lock file.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;

use crate::report::filename_timestamp;
use crate::show::{metric_summary, section_for, unit_display, SECTION_ORDER};
use crate::stats::{bootstrap_median_ci, mann_whitney_p};

// Fallback for results without measurement.direction (pre-Reporting-Spec-v1
// reports): metrics where a HIGHER median is better. Mirrors compare.py's
// HIGHER_IS_BETTER (render.throughput only — deliberately narrower than
// show.py's copy) so the direction-aware "best" verdict stays byte-identical.
fn higher_is_better_fallback(metric: &str) -> bool {
    metric == "render.throughput"
}

// Light-theme hex per entity, dataviz reference palette slots 1-8. Color is
// assigned by entity (file) index, fixed across every chart on the page.
// The report renders light-only by design (readability over ambience).
const PALETTE: &[&str] = &[
    "#2a78d6", // 1 blue
    "#008300", // 2 green
    "#e87ba4", // 3 magenta
    "#eda100", // 4 yellow
    "#1baf7a", // 5 aqua
    "#eb6834", // 6 orange
    "#4a3aa7", // 7 violet
    "#e34948", // 8 red
];

// Noise guard: below this relative-noise floor, or once the reference
// median is effectively zero, a "%" delta stops meaning anything.
const NOISE_MULT: f64 = 2.0;
const PCT_CAP: f64 = 300.0;
const BOOTSTRAP_B: usize = 1000;

type Key = (String, String, String, String); // metric, variant, shell, unit
type Cells = BTreeMap<usize, Value>; // ascending file index -> result

// --- small helpers --------------------------------------------------------

/// Python `f"{v:,.2f}"` — thousands separators + 2 decimals; None -> "-".
/// Values below 0.1 keep 5 decimals so near-zero metrics don't collide.
fn fmt(v: Option<f64>) -> String {
    match v {
        None => "-".to_string(),
        Some(v) => {
            let s = if v.abs() < 0.1 {
                format!("{v:.5}")
            } else {
                format!("{v:.2}")
            };
            let (int_part, frac) = s.split_once('.').unwrap();
            let (sign, digits) = if let Some(d) = int_part.strip_prefix('-') {
                ("-", d)
            } else {
                ("", int_part)
            };
            let mut grouped = String::new();
            for (i, c) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i) % 3 == 0 {
                    grouped.push(',');
                }
                grouped.push(c);
            }
            format!("{sign}{grouped}.{frac}")
        }
    }
}

fn fmtv(v: f64) -> String {
    fmt(Some(v))
}

/// html.escape(s, quote=True).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Middle-truncate (entity labels often end in a disambiguating suffix).
fn truncate(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return s.to_string();
    }
    let keep_end = 12.min(n / 2);
    let keep_start = n - keep_end - 1;
    let start: String = chars[..keep_start].iter().collect();
    let end: String = chars[chars.len() - keep_end..].iter().collect();
    format!("{start}\u{2026}{end}")
}

fn median(res: &Value) -> f64 {
    res["median"].as_f64().unwrap_or(0.0)
}
fn p95_of(res: &Value) -> f64 {
    res["p95"].as_f64().unwrap_or_else(|| median(res))
}
fn stddev_of(res: &Value) -> f64 {
    res["stddev"].as_f64().unwrap_or(0.0)
}
fn has_samples(res: &Value) -> bool {
    res.get("samples").map(|v| !v.is_null()).unwrap_or(false)
}
fn samples_of(res: &Value) -> Option<Vec<f64>> {
    res.get("samples")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
}
fn ci_of(res: &Value) -> Option<(f64, f64)> {
    if !has_samples(res) {
        return None;
    }
    let s = samples_of(res)?;
    bootstrap_median_ci(&s, BOOTSTRAP_B, 0)
}
fn runs_str(res: &Value) -> String {
    match res.get("runs") {
        Some(v) if v.is_u64() => v.as_u64().unwrap().to_string(),
        Some(v) if v.is_i64() => v.as_i64().unwrap().to_string(),
        Some(v) if v.is_f64() => v.as_f64().unwrap().to_string(),
        _ => String::new(),
    }
}

fn row_key(res: &Value) -> Key {
    (
        res["metric"].as_str().unwrap_or("").to_string(),
        res["variant"].as_str().unwrap_or("").to_string(),
        res["shell"].as_str().unwrap_or("").to_string(),
        res["unit"].as_str().unwrap_or("").to_string(),
    )
}

fn row_label(key: &Key) -> String {
    let mut parts = vec![key.0.clone()];
    if !key.1.is_empty() {
        parts.push(format!("[{}]", key.1));
    }
    if !key.2.is_empty() {
        parts.push(format!("({})", key.2));
    }
    parts.push(format!("({})", unit_display(&key.3)));
    parts.join(" ")
}

fn higher_is_better(metric: &str, res: &Value) -> bool {
    match res["measurement"]["direction"].as_str() {
        Some("higher-is-better") => true,
        Some("lower-is-better") => false,
        _ => higher_is_better_fallback(metric),
    }
}

fn is_better(metric: &str, a: &Value, b: &Value) -> bool {
    if higher_is_better(metric, a) {
        median(a) > median(b)
    } else {
        median(a) < median(b)
    }
}

fn best_index(metric: &str, cells: &Cells) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (&idx, res) in cells.iter() {
        match best {
            None => best = Some(idx),
            Some(b) => {
                if is_better(metric, res, &cells[&b]) {
                    best = Some(idx);
                }
            }
        }
    }
    best
}

/// Direction-aware competition ranking by exact median value.
///
/// A cell's rank is one plus the number of strictly better cells, so exact
/// ties share a rank and leave the corresponding gap (1, 1, 3). Missing
/// report cells are absent from both the input and the returned ranks.
fn competition_ranks(metric: &str, cells: &Cells) -> BTreeMap<usize, usize> {
    cells
        .iter()
        .map(|(&idx, res)| {
            let better_count = cells
                .values()
                .filter(|other| is_better(metric, other, res))
                .count();
            (idx, better_count + 1)
        })
        .collect()
}

fn delta_pct(metric: &str, best_res: &Value, other_res: &Value) -> Option<f64> {
    let best_v = median(best_res);
    let other_v = median(other_res);
    if best_v == 0.0 {
        return None;
    }
    if higher_is_better(metric, best_res) {
        Some((best_v - other_v) / best_v.abs() * 100.0)
    } else {
        Some((other_v - best_v) / best_v.abs() * 100.0)
    }
}

fn format_delta(metric: &str, better: &Value, worse: &Value, unit: &str) -> String {
    let better_v = median(better);
    let worse_v = median(worse);
    let diff = (better_v - worse_v).abs();
    let noise = NOISE_MULT * stddev_of(better).max(stddev_of(worse));
    let pct = delta_pct(metric, better, worse);
    if noise > 0.0 && diff <= noise {
        return "\u{2248} within noise".to_string();
    }
    if let Some(p) = pct {
        if better_v.abs() > 1e-9 && p.abs() <= PCT_CAP {
            return format!("+{p:.1}%");
        }
    }
    let unit_disp = if unit == "pct" {
        "%-pt"
    } else {
        unit_display(unit)
    };
    format!("\u{0394} {} {}", fmtv(diff), unit_disp)
}

fn overview_delta_pct(metric: &str, entity: &Value, baseline: &Value) -> Option<f64> {
    let base_v = median(baseline);
    let ent_v = median(entity);
    if base_v == 0.0 {
        return None;
    }
    if higher_is_better(metric, baseline) {
        Some((ent_v - base_v) / base_v.abs() * 100.0)
    } else {
        Some((base_v - ent_v) / base_v.abs() * 100.0)
    }
}

// --- data collection ------------------------------------------------------

fn column_labels(reports: &[Value]) -> Vec<String> {
    let terms: Vec<String> = reports
        .iter()
        .map(|r| {
            r["meta"]["terminal"]
                .as_str()
                .unwrap_or("unknown")
                .to_string()
        })
        .collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for t in &terms {
        *counts.entry(t.as_str()).or_insert(0) += 1;
    }
    reports
        .iter()
        .zip(&terms)
        .map(|(r, t)| {
            if counts[t.as_str()] > 1 {
                let ts = r["meta"]["timestamp"].as_str().unwrap_or("");
                format!("{t} ({ts})")
            } else {
                t.clone()
            }
        })
        .collect()
}

fn collect_rows(reports: &[Value]) -> (Vec<Key>, HashMap<Key, Cells>) {
    let mut order: Vec<Key> = Vec::new();
    let mut rows: HashMap<Key, Cells> = HashMap::new();
    for (idx, r) in reports.iter().enumerate() {
        if let Some(arr) = r["results"].as_array() {
            for res in arr {
                let key = row_key(res);
                if !rows.contains_key(&key) {
                    rows.insert(key.clone(), BTreeMap::new());
                    order.push(key.clone());
                }
                rows.get_mut(&key).unwrap().insert(idx, res.clone());
            }
        }
    }
    (order, rows)
}

fn any_samples(rows: &HashMap<Key, Cells>) -> bool {
    rows.values().flat_map(|c| c.values()).any(has_samples)
}

fn metrics_present(order: &[Key]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for key in order {
        let m = &key.0;
        if metric_summary(m).is_some() && !seen.contains(m) {
            seen.push(m.clone());
        }
    }
    seen
}

// --- markdown report ------------------------------------------------------

fn build_comparison_table(reports: &[Value], labels: &[String]) -> String {
    let (order, rows) = collect_rows(reports);
    let n = labels.len();

    let header = format!("| Metric | {} |", labels.join(" | "));
    let sep = format!("{}|", "|---".repeat(n + 1));
    let mut body = Vec::new();
    for key in &order {
        let cells = &rows[key];
        let metric = &key.0;
        let best_idx = best_index(metric, cells);
        let mut cell_strs = Vec::new();
        for idx in 0..n {
            match cells.get(&idx) {
                None => cell_strs.push("-".to_string()),
                Some(res) => {
                    let mut val = fmtv(median(res));
                    if Some(idx) == best_idx && cells.len() > 1 {
                        val = format!("**{val}**");
                    }
                    cell_strs.push(val);
                }
            }
        }
        body.push(format!(
            "| {} | {} |",
            row_label(key),
            cell_strs.join(" | ")
        ));
    }

    let samples = any_samples(&rows);
    let detail_label = if samples {
        "p95 / stddev / 95% CI (median)"
    } else {
        "p95 / stddev"
    };
    let detail_cols: Vec<String> = labels
        .iter()
        .map(|l| format!("{l} {detail_label}"))
        .collect();
    let detail_header = format!("| Metric | {} |", detail_cols.join(" | "));
    let detail_sep = format!("{}|", "|---".repeat(n + 1));
    let mut detail_body = Vec::new();
    for key in &order {
        let cells = &rows[key];
        let mut cell_strs = Vec::new();
        for idx in 0..n {
            match cells.get(&idx) {
                None => cell_strs.push("-".to_string()),
                Some(res) => {
                    let mut cell = format!("{} / {}", fmtv(p95_of(res)), fmtv(stddev_of(res)));
                    if samples {
                        match ci_of(res) {
                            Some((lo, hi)) => {
                                cell.push_str(&format!(" / [{}, {}]", fmtv(lo), fmtv(hi)))
                            }
                            None => cell.push_str(" / -"),
                        }
                    }
                    cell_strs.push(cell);
                }
            }
        }
        detail_body.push(format!(
            "| {} | {} |",
            row_label(key),
            cell_strs.join(" | ")
        ));
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(header);
    lines.push(sep);
    lines.extend(body);
    lines.push(String::new());
    lines.push(format!("### Detail ({detail_label})"));
    lines.push(String::new());
    lines.push(detail_header);
    lines.push(detail_sep);
    lines.extend(detail_body);
    if samples {
        lines.push(String::new());
        lines.push(format!(
            "95% CI is a bootstrap ({BOOTSTRAP_B} resamples) confidence interval on the \
median, computed from `--keep-samples` data; `-` means that file didn't \
keep samples for this metric."
        ));
    }
    let guide = metrics_present(&order);
    if !guide.is_empty() {
        lines.push(String::new());
        lines.push("### Metric guide".to_string());
        lines.push(String::new());
        for m in &guide {
            lines.push(format!("- `{m}` — {}", metric_summary(m).unwrap()));
        }
        lines.push(String::new());
        lines.push("Details: `docs/benchmark-metrics.md`.".to_string());
    }
    lines.join("\n")
}

// --- regression report ----------------------------------------------------

fn parse_threshold(s: &str) -> Option<f64> {
    let s = s.trim();
    let s = s.strip_suffix('%').unwrap_or(s).trim_end();
    let v = s.parse::<f64>().ok()?;
    (v.is_finite() && v >= 0.0).then_some(v / 100.0)
}

fn regression_fraction(metric: &str, baseline: &Value, latest: &Value) -> Option<f64> {
    let b = median(baseline);
    if b == 0.0 {
        return None;
    }
    let l = median(latest);
    Some(if higher_is_better(metric, baseline) {
        (b - l) / b
    } else {
        (l - b) / b
    })
}

/// Returns (markdown, exit_code). exit 3 when a real regression fails or a
/// baseline row has no counterpart in the latest report.
fn build_regression_report(reports: &[Value], labels: &[String], threshold: f64) -> (String, u8) {
    let baseline = &reports[0];
    let latest = &reports[1];
    let mut base_by_key: Vec<(Key, Value)> = Vec::new();
    if let Some(arr) = baseline["results"].as_array() {
        for r in arr {
            base_by_key.push((row_key(r), r.clone()));
        }
    }
    let mut latest_by_key: HashMap<Key, Value> = HashMap::new();
    if let Some(arr) = latest["results"].as_array() {
        for r in arr {
            latest_by_key.insert(row_key(r), r.clone());
        }
    }

    // (key, b, l, pct, sig_label, is_failing)
    let mut candidates: Vec<(Key, f64, f64, f64, String, bool)> = Vec::new();
    let mut any_fail = false;
    let mut missing: Vec<Key> = Vec::new();
    for (key, base_res) in &base_by_key {
        let Some(latest_res) = latest_by_key.get(key) else {
            missing.push(key.clone());
            continue;
        };
        let metric = &key.0;
        let Some(pct) = regression_fraction(metric, base_res, latest_res) else {
            continue;
        };
        let b = median(base_res);
        let l = median(latest_res);
        if pct <= threshold {
            continue;
        }
        let (sig_label, is_failing) = if has_samples(base_res) && has_samples(latest_res) {
            let p = mann_whitney_p(
                &samples_of(base_res).unwrap(),
                &samples_of(latest_res).unwrap(),
            );
            match p {
                Some(p) => {
                    let failing = p < 0.05;
                    (
                        format!("{} (p={p:.3})", if failing { "yes" } else { "no" }),
                        failing,
                    )
                }
                None => ("n/a".to_string(), false),
            }
        } else {
            ("n/a (no samples)".to_string(), true)
        };
        any_fail = any_fail || is_failing;
        candidates.push((key.clone(), b, l, pct, sig_label, is_failing));
    }

    let mut lines: Vec<String> = vec![
        format!(
            "## Regression check ({} -> {}, threshold {:.1}%)",
            labels[0],
            labels[1],
            threshold * 100.0
        ),
        String::new(),
        "Fail requires the threshold exceeded AND Mann-Whitney p<0.05, for rows \
where both files kept samples (`run --keep-samples`); rows without \
samples on both sides fall back to the threshold-only check."
            .to_string(),
        String::new(),
    ];
    if !missing.is_empty() {
        lines.push(
            "Baseline rows with no result in the latest report (a metric that \
disappeared cannot pass the gate):"
                .to_string(),
        );
        lines.push(String::new());
        for key in &missing {
            lines.push(format!("- {}", row_label(key)));
        }
        lines.push(String::new());
    }
    let missing_code = if missing.is_empty() { 0 } else { 3 };
    if candidates.is_empty() {
        lines.push(format!(
            "No regressions above the {:.1}% threshold.",
            threshold * 100.0
        ));
        return (lines.join("\n"), missing_code);
    }

    lines.push("| Metric | Baseline | Latest | Change | Significant? | Regression? |".to_string());
    lines.push("|---|---|---|---|---|---|".to_string());
    // sorted by -pct (descending); stable, like Python's sorted().
    let mut sorted = candidates.clone();
    sorted.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    for (key, b, l, pct, sig_label, is_failing) in &sorted {
        lines.push(format!(
            "| {} | {} | {} | +{:.1}% | {} | {} |",
            row_label(key),
            fmtv(*b),
            fmtv(*l),
            pct * 100.0,
            sig_label,
            if *is_failing { "yes" } else { "no" }
        ));
    }

    if !any_fail {
        lines.push(String::new());
        lines.push(
            "No row met both the threshold and the significance bar, so this is not treated as a regression."
                .to_string(),
        );
        return (lines.join("\n"), missing_code);
    }
    (lines.join("\n"), 3)
}

// --- baseline resolution --------------------------------------------------

enum BaselineResult {
    Idx(usize),
    Error(String),
}

fn resolve_baseline_idx(labels: &[String], files: &[String], name: Option<&str>) -> BaselineResult {
    let Some(name) = name else {
        return BaselineResult::Idx(0);
    };
    for (idx, label) in labels.iter().enumerate() {
        if label == name {
            return BaselineResult::Idx(idx);
        }
    }
    for (idx, f) in files.iter().enumerate() {
        let base = Path::new(f)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(f);
        if f == name || base == name {
            return BaselineResult::Idx(idx);
        }
    }
    let available = labels.join(", ");
    BaselineResult::Error(format!(
        "tbench: --baseline '{name}' matches no input file or terminal name. Available: {available}"
    ))
}

// --- HTML: geometry constants ---------------------------------------------

const CHART_W: f64 = 600.0;
const LABEL_COL: f64 = 150.0;
const TEXT_COL: f64 = 190.0;
const BAR_H: f64 = 24.0;
const ROW_H: f64 = 40.0;
const TOP_PAD: f64 = 10.0;
const BOT_PAD: f64 = 10.0;

const OV_LABEL_COL: f64 = 200.0;
const OV_SUBROW_H: f64 = 16.0;
const OV_HIT_H: f64 = 24.0;
const OV_BAR_H: f64 = 8.0;
const OV_GROUP_GAP: f64 = 8.0;
const OV_HEADER_H: f64 = 20.0;
const OV_BOT_PAD: f64 = 10.0;
const OV_LABEL_GAP: f64 = 6.0;
const OV_TEXT_RESERVE: f64 = 45.0;
fn ov_half_w() -> f64 {
    (CHART_W - OV_LABEL_COL) / 2.0
}
fn ov_bar_max() -> f64 {
    ov_half_w() - OV_TEXT_RESERVE
}

fn rounded_bar_path(mut x0: f64, mut x1: f64, y: f64, h: f64, r: f64, round_end: &str) -> String {
    if x1 < x0 {
        std::mem::swap(&mut x0, &mut x1);
    }
    let mut r = r;
    if x1 - x0 < 2.0 * r {
        r = 0.0f64.max((x1 - x0) / 2.0);
    }
    if r <= 0.0 {
        return format!(
            "M {x0:.2},{y:.2} L {x1:.2},{y:.2} L {x1:.2},{:.2} L {x0:.2},{:.2} Z",
            y + h,
            y + h
        );
    }
    if round_end == "min" {
        format!(
            "M {x1:.2},{y:.2} L {:.2},{y:.2} Q {x0:.2},{y:.2} {x0:.2},{:.2} L {x0:.2},{:.2} Q {x0:.2},{:.2} {:.2},{:.2} L {x1:.2},{:.2} Z",
            x0 + r,
            y + r,
            y + h - r,
            y + h,
            x0 + r,
            y + h,
            y + h
        )
    } else {
        format!(
            "M {x0:.2},{y:.2} L {:.2},{y:.2} Q {x1:.2},{y:.2} {x1:.2},{:.2} L {x1:.2},{:.2} Q {x1:.2},{:.2} {:.2},{:.2} L {x0:.2},{:.2} Z",
            x1 - r,
            y + r,
            y + h - r,
            y + h,
            x1 - r,
            y + h,
            y + h
        )
    }
}

// --- HTML: per-metric dot+interval chart ----------------------------------

fn build_bar_chart_svg(key: &Key, cells: &Cells, labels: &[String]) -> String {
    let metric = &key.0;
    let unit = &key.3;
    let n_all = labels.len();
    let present: Vec<usize> = (0..n_all).filter(|i| cells.contains_key(i)).collect();
    let n = present.len();
    let height = TOP_PAD + n as f64 * ROW_H + BOT_PAD;
    let bar_area_w = CHART_W - LABEL_COL - TEXT_COL;

    // (p95, stddev, optional bootstrap CI) per entity index.
    type RowStats = HashMap<usize, (f64, f64, Option<(f64, f64)>)>;
    let mut row_stats: RowStats = HashMap::new();
    let mut max_val = 0.0f64;
    for &i in &present {
        let res = &cells[&i];
        let m = median(res);
        let p = p95_of(res);
        let ci = ci_of(res);
        row_stats.insert(i, (m, p, ci));
        max_val = max_val.max(p).max(m).max(ci.map(|c| c.1).unwrap_or(0.0));
    }
    let scale = if max_val > 0.0 {
        bar_area_w / max_val
    } else {
        0.0
    };

    let ranks = competition_ranks(metric, cells);
    let best_idx = ranks
        .iter()
        .find_map(|(&idx, &rank)| (rank == 1).then_some(idx));
    let next_rank_idx = ranks
        .iter()
        .filter(|(_, rank)| **rank > 1)
        .min_by_key(|(_, rank)| **rank)
        .map(|(&idx, _)| idx);
    let unit_disp = unit_display(unit);

    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "<svg viewBox=\"0 0 {CHART_W} {height}\" class=\"tb-chart\" role=\"img\" aria-label=\"{} direction-aware ranking\">",
        esc(&row_label(key))
    ));
    let x0 = LABEL_COL;
    parts.push(format!(
        "<line x1=\"{x0}\" y1=\"0\" x2=\"{x0}\" y2=\"{height}\" class=\"tb-baseline\"></line>"
    ));

    for (row_i, &i) in present.iter().enumerate() {
        let res = &cells[&i];
        let y = TOP_PAD + row_i as f64 * ROW_H;
        let row_center = y + BAR_H / 2.0;
        let (m, p, ci) = row_stats[&i];
        let stddev = stddev_of(res);
        let runs = runs_str(res);
        let median_x = x0 + 0.0f64.max(m * scale);
        let p95_x = x0 + 0.0f64.max(p * scale);
        let slot = i % PALETTE.len();
        let rank = ranks[&i];
        let is_best = rank == 1 && n > 1;

        let entity_label = truncate(&labels[i], 24);
        parts.push(format!(
            "<text x=\"0\" y=\"{:.2}\" class=\"tb-entity-label\">{}<title>{}</title></text>",
            row_center + 4.0,
            esc(&entity_label),
            esc(&labels[i])
        ));

        let mut ci_attr = String::new();
        let mut ci_aria = String::new();
        let mut ci_x1 = median_x;
        let mut ci_x0 = median_x;
        if let Some((lo, hi)) = ci {
            let ci_str = format!("[{}, {}] {}", fmtv(lo), fmtv(hi), unit_disp);
            ci_attr = format!("data-ci=\"{}\" ", esc(&ci_str));
            ci_aria = format!(", 95% CI {ci_str}");
            ci_x0 = x0 + 0.0f64.max(lo * scale);
            ci_x1 = x0 + 0.0f64.max(hi * scale);
        }

        let runs_aria = if !runs.is_empty() && runs != "0" {
            format!(", {runs} runs")
        } else {
            String::new()
        };
        let best_prefix = if is_best { "Best. " } else { "" };
        let aria_label = format!(
            "{best_prefix}{}: rank {rank} of {n}, median {} {unit_disp}, p95 {} {unit_disp}, stddev {} {unit_disp}{runs_aria}{ci_aria}",
            labels[i],
            fmtv(m),
            fmtv(p),
            fmtv(stddev)
        );
        parts.push(format!(
            "<rect x=\"{x0:.2}\" y=\"{y:.2}\" width=\"{:.2}\" height=\"{BAR_H:.2}\" class=\"tb-bar\" tabindex=\"0\" aria-describedby=\"tb-tooltip\" fill=\"transparent\" data-entity=\"{}\" data-rank=\"{rank} of {n}\" data-median=\"{} {}\" data-p95=\"{} {}\" data-stddev=\"{} {}\" data-runs=\"{}\" {ci_attr}aria-label=\"{}\"></rect>",
            CHART_W - x0,
            esc(&labels[i]),
            esc(&fmtv(m)), esc(unit_disp),
            esc(&fmtv(p)), esc(unit_disp),
            esc(&fmtv(stddev)), esc(unit_disp),
            esc(&runs),
            esc(&aria_label)
        ));

        if ci.is_some() {
            let band_y = row_center - 3.0;
            parts.push(format!(
                "<rect x=\"{:.2}\" y=\"{band_y:.2}\" width=\"{:.2}\" height=\"6\" rx=\"3\" class=\"tb-interval-band\" style=\"fill:var(--series-{})\"></rect>",
                ci_x0.min(ci_x1),
                (ci_x1 - ci_x0).abs(),
                slot + 1
            ));
        } else if (p95_x - median_x).abs() > 0.5 {
            parts.push(format!(
                "<line x1=\"{median_x:.2}\" y1=\"{row_center:.2}\" x2=\"{p95_x:.2}\" y2=\"{row_center:.2}\" class=\"tb-whisker\" style=\"stroke:var(--series-{})\"></line>",
                slot + 1
            ));
            parts.push(format!(
                "<line x1=\"{p95_x:.2}\" y1=\"{:.2}\" x2=\"{p95_x:.2}\" y2=\"{:.2}\" class=\"tb-whisker\" style=\"stroke:var(--series-{})\"></line>",
                y - 2.0,
                y + BAR_H + 2.0,
                slot + 1
            ));
        }

        parts.push(format!(
            "<circle cx=\"{median_x:.2}\" cy=\"{row_center:.2}\" r=\"6\" class=\"tb-median-dot\" style=\"fill:var(--series-{})\"></circle>",
            slot + 1
        ));

        let label_x = median_x.max(p95_x).max(ci_x1) + 8.0;
        let val_text = format!("#{rank} \u{00b7} {} {unit_disp}", fmtv(m));
        parts.push(format!(
            "<text x=\"{label_x:.2}\" y=\"{:.2}\" class=\"tb-value-label\">{}</text>",
            row_center + 4.0,
            esc(&val_text)
        ));

        let (delta_text, delta_class): (Option<String>, &str) = if is_best {
            match next_rank_idx {
                Some(next) => {
                    let margin = format_delta(metric, res, &cells[&next], unit);
                    (
                        Some(format!("\u{2605} best \u{00b7} {margin} vs next rank")),
                        "tb-best-badge",
                    )
                }
                None => (Some("\u{2605} best".to_string()), "tb-best-badge"),
            }
        } else if let Some(bi) = best_idx.filter(|_| n > 1) {
            let gap = format_delta(metric, &cells[&bi], res, unit);
            (
                Some(format!("rank {rank} \u{00b7} {gap} vs best")),
                "tb-delta-badge",
            )
        } else {
            (None, "")
        };

        if let Some(dt) = delta_text {
            parts.push(format!(
                "<text x=\"{label_x:.2}\" y=\"{:.2}\" class=\"{delta_class}\">{}</text>",
                row_center + 16.0,
                esc(&dt)
            ));
        }
    }

    parts.push("</svg>".to_string());
    parts.join("")
}

// --- HTML: overview diverging-delta chart ---------------------------------

fn build_overview_svg(
    order: &[Key],
    rows: &HashMap<Key, Cells>,
    labels: &[String],
    baseline_idx: usize,
) -> String {
    // groups: (key, baseline_res, [(idx, kind, text, pct_opt, entity_res)])
    struct Entry {
        idx: usize,
        kind: &'static str,
        text: String,
        pct: Option<f64>,
        res: Value,
    }
    let mut groups: Vec<(Key, Value, Vec<Entry>)> = Vec::new();
    let mut reliable_abs: Vec<f64> = Vec::new();
    for key in order {
        let cells = &rows[key];
        if !cells.contains_key(&baseline_idx) || cells.len() <= 1 {
            continue;
        }
        let baseline_res = cells[&baseline_idx].clone();
        let metric = &key.0;
        let unit = &key.3;
        let mut entries: Vec<Entry> = Vec::new();
        for idx in 0..labels.len() {
            if idx == baseline_idx || !cells.contains_key(&idx) {
                continue;
            }
            let entity_res = cells[&idx].clone();
            let pct = overview_delta_pct(metric, &entity_res, &baseline_res);
            let diff = (median(&entity_res) - median(&baseline_res)).abs();
            let noise = NOISE_MULT * stddev_of(&entity_res).max(stddev_of(&baseline_res));
            if noise > 0.0 && diff <= noise {
                entries.push(Entry {
                    idx,
                    kind: "noise",
                    text: "\u{2248}".to_string(),
                    pct: None,
                    res: entity_res,
                });
            } else if pct.is_some()
                && median(&baseline_res).abs() > 1e-9
                && pct.unwrap().abs() <= PCT_CAP
            {
                let p = pct.unwrap();
                entries.push(Entry {
                    idx,
                    kind: "pct",
                    text: format!("{p:+.1}%"),
                    pct: Some(p),
                    res: entity_res,
                });
                reliable_abs.push(p.abs());
            } else {
                let unit_disp = if unit == "pct" {
                    "%-pt"
                } else {
                    unit_display(unit)
                };
                let diff_signed = median(&entity_res) - median(&baseline_res);
                let sign = if diff_signed >= 0.0 { "+" } else { "-" };
                let text = format!("\u{0394} {sign}{} {unit_disp}", fmtv(diff_signed.abs()));
                entries.push(Entry {
                    idx,
                    kind: "abs",
                    text,
                    pct: None,
                    res: entity_res,
                });
            }
        }
        if !entries.is_empty() {
            groups.push((key.clone(), baseline_res, entries));
        }
    }

    if groups.is_empty() {
        return String::new();
    }

    // Guard the known divide-by-zero: when every reliable delta is 0 (or
    // none are reliable), fall back to a zero scale (stubs) instead of inf.
    let max_reliable = reliable_abs.iter().cloned().fold(0.0f64, f64::max);
    let scale = if max_reliable > 0.0 {
        ov_bar_max() / max_reliable
    } else {
        0.0
    };
    let mid_x = OV_LABEL_COL + (CHART_W - OV_LABEL_COL) / 2.0;

    let mut height = OV_HEADER_H;
    let mut group_tops: Vec<f64> = Vec::new();
    for (_k, _b, entries) in &groups {
        group_tops.push(height);
        height += entries.len() as f64 * OV_SUBROW_H + OV_GROUP_GAP;
    }
    height += OV_BOT_PAD;

    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "<svg viewBox=\"0 0 {CHART_W} {height:.2}\" class=\"tb-chart tb-wide-chart\" style=\"max-width:{CHART_W}px\" role=\"img\" aria-label=\"Overview: every metric vs baseline {}\">",
        esc(&labels[baseline_idx])
    ));
    parts.push(format!(
        "<line x1=\"{mid_x:.2}\" y1=\"{OV_HEADER_H}\" x2=\"{mid_x:.2}\" y2=\"{height:.2}\" class=\"tb-baseline\"></line>"
    ));
    parts.push(format!(
        "<text x=\"{mid_x:.2}\" y=\"{:.2}\" class=\"tb-overview-baseline-label\" text-anchor=\"middle\">baseline: {}</text>",
        OV_HEADER_H - 6.0,
        esc(&labels[baseline_idx])
    ));

    for ((key, _baseline_res, entries), &top) in groups.iter().zip(&group_tops) {
        let unit = &key.3;
        let group_h = entries.len() as f64 * OV_SUBROW_H;
        let unit_disp = unit_display(unit);
        parts.push(format!(
            "<text x=\"0\" y=\"{:.2}\" class=\"tb-entity-label\">{}</text>",
            top + group_h / 2.0 + 4.0,
            esc(&row_label(key))
        ));
        for (row_i, e) in entries.iter().enumerate() {
            let y_center = top + row_i as f64 * OV_SUBROW_H + OV_SUBROW_H / 2.0;
            let slot = e.idx % PALETTE.len();
            let m = median(&e.res);
            let p = p95_of(&e.res);
            let stddev = stddev_of(&e.res);
            let runs = runs_str(&e.res);

            let ci = ci_of(&e.res);
            let ci_attr = match ci {
                Some((lo, hi)) => format!(
                    "data-ci=\"{}\" ",
                    esc(&format!("[{}, {}] {unit_disp}", fmtv(lo), fmtv(hi)))
                ),
                None => String::new(),
            };

            let aria_label = format!(
                "{} vs baseline {} on {}: {}, median {} {unit_disp}",
                labels[e.idx],
                labels[baseline_idx],
                row_label(key),
                e.text,
                fmtv(m)
            );
            parts.push(format!(
                "<rect x=\"{OV_LABEL_COL:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{OV_HIT_H}\" class=\"tb-bar\" tabindex=\"0\" aria-describedby=\"tb-tooltip\" fill=\"transparent\" data-entity=\"{}\" data-median=\"{} {}\" data-p95=\"{} {}\" data-stddev=\"{} {}\" data-runs=\"{}\" data-delta=\"{} vs {}\" {ci_attr}aria-label=\"{}\"></rect>",
                y_center - OV_HIT_H / 2.0,
                CHART_W - OV_LABEL_COL,
                esc(&labels[e.idx]),
                esc(&fmtv(m)), esc(unit_disp),
                esc(&fmtv(p)), esc(unit_disp),
                esc(&fmtv(stddev)), esc(unit_disp),
                esc(&runs),
                esc(&e.text), esc(&labels[baseline_idx]),
                esc(&aria_label)
            ));

            let (label_x, anchor) = match (e.kind, e.pct) {
                ("pct", Some(pct)) if scale > 0.0 => {
                    let bar_len = pct.abs() * scale;
                    let (bx0, bx1, round_end) = if pct >= 0.0 {
                        (mid_x, mid_x + bar_len, "max")
                    } else {
                        (mid_x - bar_len, mid_x, "min")
                    };
                    let path = rounded_bar_path(
                        bx0,
                        bx1,
                        y_center - OV_BAR_H / 2.0,
                        OV_BAR_H,
                        4.0,
                        round_end,
                    );
                    parts.push(format!(
                    "<path d=\"{path}\" class=\"tb-overview-bar\" style=\"fill:var(--series-{})\"></path>",
                    slot + 1
                ));
                    if pct >= 0.0 {
                        (bx1 + OV_LABEL_GAP, "start")
                    } else {
                        (bx0 - OV_LABEL_GAP, "end")
                    }
                }
                _ => {
                    parts.push(format!(
                    "<line x1=\"{:.2}\" y1=\"{y_center:.2}\" x2=\"{:.2}\" y2=\"{y_center:.2}\" class=\"tb-overview-stub\"></line>",
                    mid_x - 5.0,
                    mid_x + 5.0
                ));
                    (mid_x + OV_LABEL_GAP, "start")
                }
            };

            parts.push(format!(
                "<circle cx=\"{mid_x:.2}\" cy=\"{y_center:.2}\" r=\"3\" class=\"tb-overview-dot\" style=\"fill:var(--series-{})\"></circle>",
                slot + 1
            ));
            parts.push(format!(
                "<text x=\"{label_x:.2}\" y=\"{:.2}\" text-anchor=\"{anchor}\" class=\"tb-value-label\">{}</text>",
                y_center + 4.0,
                esc(&e.text)
            ));
        }
    }

    parts.push("</svg>".to_string());
    parts.join("")
}

// --- HTML: at-a-glance results table --------------------------------------
//
// Direction-aware verdict per metric, rendered as plain HTML so the outcome
// reads without hovering: the best cell is green-highlighted, every other
// cell carries a "N% behind" chip (or "within noise" when the gap is below
// the noise guard).

fn build_glance_table(order: &[Key], rows: &HashMap<Key, Cells>, labels: &[String]) -> String {
    if order.is_empty() {
        return String::new();
    }
    let header: String = labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "<th scope=\"col\"><span class=\"tb-swatch\" style=\"background:var(--series-{})\"></span>{}</th>",
                i % PALETTE.len() + 1,
                esc(l)
            )
        })
        .collect();
    let mut body = String::new();
    for key in order {
        let cells = &rows[key];
        let ranks = competition_ranks(&key.0, cells);
        let best = ranks
            .iter()
            .find_map(|(&idx, &rank)| (rank == 1).then_some(idx));
        let unit_disp = unit_display(&key.3);
        let mut tds = String::new();
        for idx in 0..labels.len() {
            match cells.get(&idx) {
                None => tds.push_str(
                    "<td class=\"tb-glance-miss\" aria-label=\"No result\">\u{2013}</td>",
                ),
                Some(res) => {
                    let rank = ranks[&idx];
                    let rank_html = format!(
                        "<span class=\"tb-rank\">Rank {rank} of {}</span>",
                        cells.len()
                    );
                    let val = if unit_disp.is_empty() {
                        fmtv(median(res))
                    } else {
                        format!("{} {unit_disp}", fmtv(median(res)))
                    };
                    if rank == 1 && cells.len() > 1 {
                        tds.push_str(&format!(
                            "<td class=\"tb-glance-best\"><span class=\"tb-glance-val\">{}</span>{rank_html}<span class=\"tb-chip tb-chip-best\">\u{2605} best</span></td>",
                            esc(&val),
                        ));
                    } else if let Some(bi) = best.filter(|_| cells.len() > 1) {
                        let delta = format_delta(&key.0, &cells[&bi], res, &key.3);
                        let (chip_class, chip_text) = if let Some(pct) = delta.strip_prefix('+') {
                            ("tb-chip-behind".to_string(), format!("{pct} behind"))
                        } else if delta.starts_with('\u{2248}') {
                            (
                                "tb-chip-noise".to_string(),
                                "\u{2248} within noise".to_string(),
                            )
                        } else {
                            ("tb-chip-behind".to_string(), format!("{delta} behind"))
                        };
                        tds.push_str(&format!(
                            "<td><span class=\"tb-glance-val\">{}</span>{rank_html}<span class=\"tb-chip {chip_class}\">{}</span></td>",
                            esc(&val),
                            esc(&chip_text)
                        ));
                    } else {
                        tds.push_str(&format!(
                            "<td><span class=\"tb-glance-val\">{}</span>{rank_html}</td>",
                            esc(&val)
                        ));
                    }
                }
            }
        }
        body.push_str(&format!(
            "<tr><th scope=\"row\">{}</th>{tds}</tr>",
            esc(&row_label(key))
        ));
    }
    format!(
        "<table class=\"tb-glance\"><thead><tr><th scope=\"col\">Metric</th>{header}</tr></thead><tbody>{body}</tbody></table>"
    )
}

// --- HTML: full document --------------------------------------------------

fn build_html_report(
    reports: &[Value],
    labels: &[String],
    source_files: &[String],
    baseline_idx: usize,
) -> String {
    let (order, rows) = collect_rows(reports);

    let mut sections: HashMap<&str, Vec<Key>> = HashMap::new();
    for (name, _) in SECTION_ORDER {
        sections.insert(name, Vec::new());
    }
    for key in &order {
        sections
            .get_mut(section_for(&key.0))
            .unwrap()
            .push(key.clone());
    }

    let legend_items: String = (0..labels.len())
        .map(|i| {
            format!(
                "<div class=\"tb-legend-item\"><span class=\"tb-swatch\" style=\"background:var(--series-{})\"></span><span>{}</span></div>",
                i % PALETTE.len() + 1,
                esc(&labels[i])
            )
        })
        .collect();
    let legend_glyphs = "<div class=\"tb-legend-glyphs\"><span class=\"tb-legend-glyph\"><span class=\"tb-glyph-star\">\u{2605}</span> best = direction-aware best median</span><span class=\"tb-legend-glyph\">#N / Rank N = direction-aware competition rank (exact ties share a rank)</span><span class=\"tb-legend-glyph\"><span class=\"tb-glyph-dot\"></span> dot = median \u{00b7} band/whisker = 95% CI (or median \u{2192} p95 without samples)</span><span class=\"tb-legend-glyph\">hover/focus a mark for rank, median, p95, stddev, runs (and 95% CI)</span><span class=\"tb-legend-glyph\">median = typical trial \u{00b7} p95 = 95th percentile of the retained trials (the slow tail only for lower-is-better metrics) \u{00b7} stddev = spread across trials within one run, not across runs</span></div>";

    let overview_svg = build_overview_svg(&order, &rows, labels, baseline_idx);
    let overview_html = if !overview_svg.is_empty() {
        format!(
            "<section class=\"tb-card tb-section tb-section-wide\"><h2>Overview vs baseline</h2><p class=\"tb-section-subtitle\">Every metric vs the baseline file ({}); right = better, left = worse. Bars keep each entity's legend color; \u{2248} or \u{0394} marks a gap the noise guard can't trust as a clean percentage.</p><figure class=\"tb-chart-card tb-chart-card-wide\">{overview_svg}</figure></section>",
            esc(&labels[baseline_idx])
        )
    } else {
        String::new()
    };

    let glance_table = build_glance_table(&order, &rows, labels);
    let glance_html = if !glance_table.is_empty() {
        format!(
            "<section class=\"tb-card tb-section\"><h2>Results at a glance</h2><p class=\"tb-section-subtitle\">Median and direction-aware competition rank per metric (exact ties share a rank). Green = best; \u{201c}behind\u{201d} = how far that file trails the best; \u{2248} = gap within the noise guard.</p><div class=\"tb-glance-wrap\">{glance_table}</div></section>"
        )
    } else {
        String::new()
    };

    let mut section_html: Vec<String> = Vec::new();
    for (name, title) in SECTION_ORDER {
        let keys = &sections[name];
        if keys.is_empty() {
            continue;
        }
        let charts: String = keys
            .iter()
            .map(|key| {
                format!(
                    "<figure class=\"tb-chart-card\"><figcaption>{}</figcaption>{}</figure>",
                    esc(&row_label(key)),
                    build_bar_chart_svg(key, &rows[key], labels)
                )
            })
            .collect();
        let subtitle = if *name == "other" {
            "<p class=\"tb-section-subtitle\">Metrics that don\u{2019}t map to a section above (unclassified).</p>"
        } else {
            ""
        };
        section_html.push(format!(
            "<section class=\"tb-card tb-section\"><h2>{}</h2>{subtitle}<div class=\"tb-chart-grid\">{charts}</div></section>",
            esc(title)
        ));
    }

    let samples = any_samples(&rows);

    let table_header = format!(
        "<tr><th scope=\"col\">Metric</th>{}</tr>",
        labels
            .iter()
            .map(|l| format!("<th scope=\"col\">{}</th>", esc(l)))
            .collect::<String>()
    );
    let mut table_rows: Vec<String> = Vec::new();
    for key in &order {
        let cells = &rows[key];
        let ranks = competition_ranks(&key.0, cells);
        let mut cell_cells = String::new();
        for idx in 0..labels.len() {
            match cells.get(&idx) {
                None => cell_cells.push_str("<td aria-label=\"No result\">\u{2013}</td>"),
                Some(res) => {
                    let rank = ranks[&idx];
                    let mut cell_text = format!(
                        "Rank {rank} of {} \u{00b7} {} / {} / {}",
                        cells.len(),
                        fmtv(median(res)),
                        fmtv(p95_of(res)),
                        fmtv(stddev_of(res))
                    );
                    if samples {
                        match ci_of(res) {
                            Some((lo, hi)) => {
                                cell_text.push_str(&format!(" / [{}, {}]", fmtv(lo), fmtv(hi)))
                            }
                            None => cell_text.push_str(" / \u{2013}"),
                        }
                    }
                    let cell_class = if rank == 1 && cells.len() > 1 {
                        " class=\"tb-best-cell\""
                    } else {
                        ""
                    };
                    cell_cells.push_str(&format!("<td{cell_class}>{}</td>", esc(&cell_text)));
                }
            }
        }
        table_rows.push(format!(
            "<tr><td>{}</td>{cell_cells}</tr>",
            esc(&row_label(key))
        ));
    }

    let table_caption = if samples {
        format!(
            "Cells: direction-aware competition rank \u{00b7} median / p95 / stddev / 95% CI (median, bootstrap {BOOTSTRAP_B} resamples from --keep-samples data; \u{2013} within a result means that file didn't keep samples, while a standalone \u{2013} means no result and receives no rank)."
        )
    } else {
        "Cells: direction-aware competition rank \u{00b7} median / p95 / stddev. Exact median ties share a rank; \u{2013} means no result and receives no rank."
            .to_string()
    };

    let guide_metrics = metrics_present(&order);
    let guide_html = if !guide_metrics.is_empty() {
        let guide_items: String = guide_metrics
            .iter()
            .map(|m| {
                format!(
                    "<li><code>{}</code> — {}</li>",
                    esc(m),
                    esc(metric_summary(m).unwrap())
                )
            })
            .collect();
        format!(
            "<section class=\"tb-card tb-section\"><h2>Metric guide</h2><p class=\"tb-section-subtitle\">What each metric actually measures (one line each; full detail and caveats in <code>docs/benchmark-metrics.md</code>).</p><ul class=\"tb-metric-guide\">{guide_items}</ul></section>"
        )
    } else {
        String::new()
    };

    let palette_vars_light: String = (0..labels.len())
        .map(|i| {
            format!(
                "      --series-{}: {};\n",
                i + 1,
                PALETTE[i % PALETTE.len()]
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let palette_vars_light = palette_vars_light.trim_end_matches('\n').to_string();

    let css = CSS_TEMPLATE.replace("__PALETTE_LIGHT__", &palette_vars_light);

    let sources: String = source_files
        .iter()
        .map(|f| esc(f))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>tbench compare</title>\n<style>\n{css}\n</style>\n</head>\n<body class=\"viz-root\">\n<h1>tbench compare</h1>\n<div class=\"tb-meta\">Sources: {sources}</div>\n{glance_html}\n<div class=\"tb-legend\">{legend_items}</div>\n{legend_glyphs}\n{overview_html}\n{sections}\n<section class=\"tb-card tb-section\">\n  <h2>Table view</h2>\n  <table class=\"tb-table\">\n    <caption class=\"tb-table-caption\">{table_caption}</caption>\n    <thead>{table_header}</thead>\n    <tbody>{table_body}</tbody>\n  </table>\n</section>\n{guide_html}\n<div id=\"tb-tooltip\" role=\"tooltip\"></div>\n<script>\n{SCRIPT}\n</script>\n</body>\n</html>\n",
        css = css,
        sources = sources,
        glance_html = glance_html,
        legend_items = legend_items,
        legend_glyphs = legend_glyphs,
        overview_html = overview_html,
        sections = section_html.join(""),
        table_caption = esc(&table_caption),
        table_header = table_header,
        table_body = table_rows.join(""),
        guide_html = guide_html,
        SCRIPT = SCRIPT,
    )
}

// --- entry point ----------------------------------------------------------

pub fn cmd_compare(args: &[String], root: &Path) -> ExitCode {
    let mut format = "markdown".to_string();
    let mut output: Option<PathBuf> = None;
    let mut threshold: Option<String> = None;
    let mut baseline: Option<String> = None;
    let mut files: Vec<String> = Vec::new();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--format" => match it.next() {
                Some(v) => format = v.clone(),
                None => return die("tbench: --format requires a value", 2),
            },
            "--output" => match it.next() {
                Some(v) => output = Some(PathBuf::from(v)),
                None => return die("tbench: --output requires a value", 2),
            },
            "--threshold" => match it.next() {
                Some(v) => threshold = Some(v.clone()),
                None => return die("tbench: --threshold requires a value", 2),
            },
            "--baseline" => match it.next() {
                Some(v) => baseline = Some(v.clone()),
                None => return die("tbench: --baseline requires a value", 2),
            },
            other if other.starts_with("--") => {
                return die(&format!("tbench: unknown option '{other}'"), 2)
            }
            _ => files.push(arg.clone()),
        }
    }

    if files.len() < 2 {
        return die("tbench: compare requires at least 2 JSON files", 2);
    }
    for f in &files {
        if !Path::new(f).is_file() {
            return die(&format!("tbench: file not found: {f}"), 2);
        }
    }
    if format != "markdown" && format != "html" {
        return die(
            &format!("tbench: unsupported --format '{format}' (supported: markdown, html)"),
            2,
        );
    }
    let threshold_value = match threshold.as_deref() {
        None => None,
        Some(raw) => match parse_threshold(raw) {
            Some(v) => Some(v),
            None => {
                return die(
                    &format!(
                        "tbench: --threshold must be a non-negative percentage such as 10%, got '{raw}'"
                    ),
                    2,
                )
            }
        },
    };
    if threshold.is_some() && files.len() != 2 {
        return die(
            "tbench: --threshold requires exactly 2 files (baseline latest)",
            2,
        );
    }

    let mut reports: Vec<Value> = Vec::new();
    for f in &files {
        match std::fs::read_to_string(f)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
        {
            Ok(v) => reports.push(v),
            Err(e) => return die(&format!("tbench: failed to read {f}: {e}"), 2),
        }
    }

    let labels = column_labels(&reports);
    let baseline_idx = match resolve_baseline_idx(&labels, &files, baseline.as_deref()) {
        BaselineResult::Idx(i) => i,
        BaselineResult::Error(msg) => return die(&msg, 2),
    };

    let mut exit_code: u8 = 0;
    let mut reg_text: Option<String> = None;
    if let Some(threshold) = threshold_value {
        let (text, code) = build_regression_report(&reports, &labels, threshold);
        reg_text = Some(text);
        exit_code = code;
    }

    if format == "html" {
        let report = build_html_report(&reports, &labels, &files, baseline_idx);
        let out_path = output
            .unwrap_or_else(|| root.join(format!("results/compare-{}.html", filename_timestamp())));
        if let Some(dir) = out_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&out_path, &report) {
            return die(
                &format!("tbench: failed to write {}: {e}", out_path.display()),
                1,
            );
        }
        eprintln!("tbench: wrote {}", out_path.display());
        if let Some(text) = &reg_text {
            eprintln!("{text}");
        }
    } else {
        let mut out = build_comparison_table(&reports, &labels);
        if let Some(text) = &reg_text {
            out.push_str("\n\n");
            out.push_str(text);
        }
        if let Some(out_path) = &output {
            if let Some(dir) = out_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(out_path, format!("{out}\n")) {
                return die(
                    &format!("tbench: failed to write {}: {e}", out_path.display()),
                    1,
                );
            }
            eprintln!("tbench: wrote {}", out_path.display());
        }
        println!("{out}");
    }

    ExitCode::from(exit_code)
}

fn die(msg: &str, code: u8) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(code)
}

// --- static assets (CSS + tooltip JS), ported verbatim from compare.py ----

const CSS_TEMPLATE: &str = r#"  :root, .viz-root {
    color-scheme: light;
    --page: #f4f5f7;
    --surface-1: #ffffff;
    --text-primary: #191c20;
    --text-secondary: #5b6470;
    --line: #e4e6ea;
    --focus-ring: #2a78d6;
    --good-bg: #e0f2e5;
    --good-text: #156a2e;
    --bad-bg: #fdeae6;
    --bad-text: #b3402a;
    --noise-bg: #eef0f2;
    --noise-text: #5b6470;
    --fs-xs: 10px;
    --fs-sm: 11px;
    --fs-base: 12px;
    --fs-md: 13px;
    --fs-lg: 14px;
    --fs-xl: 16px;
    --fs-2xl: 22px;
    --space-1: 4px;
    --space-2: 6px;
    --space-3: 8px;
    --space-4: 10px;
    --space-5: 12px;
    --space-6: 14px;
    --space-7: 16px;
    --space-8: 24px;
    --space-9: 28px;
    --radius-sm: 2px;
    --radius-md: 6px;
    --radius-lg: 12px;
__PALETTE_LIGHT__
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    padding: var(--space-8);
    background: var(--page);
    color: var(--text-primary);
    font: var(--fs-lg)/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    max-width: 1240px;
    margin-inline: auto;
  }
  h1 { font-size: var(--fs-2xl); margin: 0 0 var(--space-1); }
  h2 { font-size: var(--fs-xl); margin: 0 0 var(--space-3); color: var(--text-primary); }
  .tb-meta { color: var(--text-secondary); font-size: var(--fs-base); margin-bottom: var(--space-7); }
  .tb-card {
    background: var(--surface-1); border: 1px solid var(--line); border-radius: var(--radius-lg);
    padding: var(--space-7) var(--space-8); margin-bottom: var(--space-7);
    box-shadow: 0 1px 2px rgba(25, 28, 32, 0.04);
  }
  .tb-glance-wrap { overflow-x: auto; }
  table.tb-glance { border-collapse: collapse; width: 100%; font-size: var(--fs-base); font-variant-numeric: tabular-nums; }
  table.tb-glance th, table.tb-glance td { text-align: left; padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--line); vertical-align: middle; }
  table.tb-glance thead th { color: var(--text-secondary); font-weight: 600; white-space: nowrap; }
  table.tb-glance thead th .tb-swatch { margin-right: var(--space-2); vertical-align: -1px; }
  table.tb-glance tbody th { font-weight: 400; color: var(--text-secondary); }
  table.tb-glance td .tb-glance-val { display: block; font-weight: 600; white-space: nowrap; }
  .tb-rank { display: block; color: var(--text-secondary); font-size: var(--fs-sm); white-space: nowrap; }
  td.tb-glance-best { background: var(--good-bg); }
  td.tb-glance-miss { color: var(--text-secondary); }
  .tb-chip {
    display: inline-block; margin-top: 2px; padding: 0 var(--space-2); border-radius: 9px;
    font-size: var(--fs-sm); white-space: nowrap;
  }
  .tb-chip-best { background: transparent; color: var(--good-text); font-weight: 600; padding: 0; }
  .tb-chip-behind { background: var(--bad-bg); color: var(--bad-text); }
  .tb-chip-noise { background: var(--noise-bg); color: var(--noise-text); }
  .tb-legend { display: flex; flex-wrap: wrap; gap: var(--space-6); padding: var(--space-4) 0; }
  .tb-legend-item { display: flex; align-items: center; gap: var(--space-2); font-size: var(--fs-md); color: var(--text-primary); }
  .tb-swatch { width: 12px; height: 12px; border-radius: var(--radius-sm); display: inline-block; }
  .tb-legend-glyphs {
    display: flex; flex-wrap: wrap; gap: var(--space-6); padding: 0 0 var(--space-4);
    font-size: var(--fs-base); color: var(--text-secondary);
  }
  .tb-legend-glyph { display: flex; align-items: center; gap: var(--space-2); }
  .tb-glyph-star { color: var(--text-primary); }
  .tb-glyph-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--text-secondary); }
  .tb-section-subtitle { color: var(--text-secondary); font-size: var(--fs-base); margin: 0 0 var(--space-4); }
  .tb-section-wide .tb-chart-card-wide { padding: var(--space-4) 0; overflow-x: auto; }
  .tb-chart-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(560px, 1fr)); gap: var(--space-7); }
  .tb-chart-card { margin: 0; padding: var(--space-4) var(--space-5); }
  .tb-chart-card figcaption { font-size: var(--fs-base); color: var(--text-secondary); margin-bottom: var(--space-1); font-variant-numeric: tabular-nums; }
  .tb-chart { width: 100%; max-width: 680px; height: auto; overflow: visible; }
  .tb-entity-label { font-size: var(--fs-sm); fill: var(--text-secondary); dominant-baseline: middle; }
  .tb-value-label { font-size: var(--fs-sm); fill: var(--text-primary); font-variant-numeric: tabular-nums; }
  .tb-best-badge { font-size: var(--fs-xs); fill: var(--good-text); font-weight: 600; }
  .tb-delta-badge { font-size: var(--fs-xs); fill: var(--text-secondary); }
  .tb-baseline { stroke: var(--line); stroke-width: 1; }
  .tb-whisker { stroke-width: 2; }
  .tb-median-dot { stroke: var(--surface-1); stroke-width: 2; }
  .tb-interval-band { opacity: 0.3; }
  .tb-overview-baseline-label { font-size: var(--fs-sm); fill: var(--text-secondary); }
  .tb-overview-stub { stroke: var(--text-secondary); stroke-width: 2; opacity: 0.6; }
  .tb-overview-dot { stroke: var(--surface-1); stroke-width: 2; }
  .tb-bar { cursor: pointer; }
  .tb-bar:hover { opacity: 0.82; }
  .tb-bar:focus { outline: none; }
  .tb-bar:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; opacity: 0.82; }
  .tb-bar[fill="transparent"]:hover ~ .tb-interval-band,
  .tb-bar[fill="transparent"]:focus-visible ~ .tb-interval-band { opacity: 0.55; }
  .tb-bar[fill="transparent"]:hover ~ .tb-median-dot,
  .tb-bar[fill="transparent"]:focus-visible ~ .tb-median-dot { stroke-width: 3; }
  .tb-bar[fill="transparent"]:hover ~ .tb-overview-bar,
  .tb-bar[fill="transparent"]:focus-visible ~ .tb-overview-bar,
  .tb-bar[fill="transparent"]:hover ~ .tb-overview-dot,
  .tb-bar[fill="transparent"]:focus-visible ~ .tb-overview-dot { opacity: 0.82; }
  table.tb-table { border-collapse: collapse; width: 100%; margin-top: var(--space-3); font-size: var(--fs-base); font-variant-numeric: tabular-nums; }
  table.tb-table caption {
    caption-side: top; text-align: left; color: var(--text-secondary); font-size: var(--fs-base);
    padding-bottom: var(--space-3);
  }
  table.tb-table th, table.tb-table td { text-align: left; padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--line); }
  table.tb-table th { color: var(--text-secondary); font-weight: 600; }
  table.tb-table td.tb-best-cell { font-weight: 600; background: var(--good-bg); }
  ul.tb-metric-guide { color: var(--text-secondary); font-size: var(--fs-base); padding-left: 1.2em; margin: 0; display: grid; gap: var(--space-3); }
  ul.tb-metric-guide code { color: var(--text-primary); }
  #tb-tooltip {
    position: absolute; display: none; pointer-events: none; z-index: 10;
    background: var(--text-primary); color: var(--surface-1);
    border-radius: var(--radius-md); padding: var(--space-4) var(--space-5); font-size: var(--fs-base); min-width: 160px;
  }
  #tb-tooltip.visible { display: block; }
  .tb-tooltip-row { display: flex; justify-content: space-between; gap: var(--space-5); }
  .tb-tooltip-k { opacity: 0.75; }
  .tb-tooltip-v { font-weight: 600; font-variant-numeric: tabular-nums; }"#;

const SCRIPT: &str = r#"(function () {
  var tooltip = document.getElementById('tb-tooltip');
  function fillTooltip(el) {
    var d = el.dataset;
    tooltip.textContent = '';
    var rows = [['Entity', d.entity]];
    if (d.rank) {
      rows.push(['Rank', d.rank]);
    }
    rows.push(
      ['Median', d.median],
      ['P95', d.p95],
      ['Stddev', d.stddev],
      ['Runs', d.runs]
    );
    if (d.delta) {
      rows.push(['vs baseline', d.delta]);
    }
    if (d.ratio) {
      rows.push(['vs best', d.ratio]);
    }
    if (d.ci) {
      rows.push(['95% CI (median)', d.ci]);
    }
    rows.forEach(function (pair) {
      var row = document.createElement('div');
      row.className = 'tb-tooltip-row';
      var k = document.createElement('span');
      k.className = 'tb-tooltip-k';
      k.textContent = pair[0];
      var v = document.createElement('span');
      v.className = 'tb-tooltip-v';
      v.textContent = pair[1];
      row.appendChild(k);
      row.appendChild(v);
      tooltip.appendChild(row);
    });
  }
  function position(el, evt) {
    var rect = el.getBoundingClientRect();
    var x = evt && typeof evt.clientX === 'number' ? evt.clientX : rect.left + rect.width / 2;
    var y = rect.top;
    tooltip.style.left = (x + window.scrollX + 14) + 'px';
    tooltip.style.top = (y + window.scrollY - 8) + 'px';
  }
  function show(el, evt) {
    fillTooltip(el);
    tooltip.classList.add('visible');
    position(el, evt);
  }
  function hide() {
    tooltip.classList.remove('visible');
  }
  document.querySelectorAll('.tb-bar').forEach(function (el) {
    el.addEventListener('pointerenter', function (evt) { show(el, evt); });
    el.addEventListener('pointermove', function (evt) { position(el, evt); });
    el.addEventListener('pointerleave', hide);
    el.addEventListener('focus', function (evt) { show(el, evt); });
    el.addEventListener('blur', hide);
    el.addEventListener('keydown', function (evt) {
      if (evt.key === 'Escape') hide();
    });
  });
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(fmtv(1234.5), "1,234.50");
        assert_eq!(fmtv(4.966), "4.97");
        assert_eq!(fmtv(-1234.5), "-1,234.50");
        assert_eq!(fmtv(0.0), "0.00000");
        assert_eq!(fmtv(0.0123), "0.01230");
        assert_eq!(fmtv(0.0123456), "0.01235");
        assert_eq!(fmtv(0.1), "0.10");
        assert_eq!(fmt(None), "-");
        assert_eq!(fmtv(1234567.891), "1,234,567.89");
    }

    #[test]
    fn row_label_building() {
        let k = (
            "render.throughput".into(),
            "plain".into(),
            "".into(),
            "MBps".into(),
        );
        assert_eq!(row_label(&k), "render.throughput [plain] (MB/s)");
        let k2 = (
            "shell.startup".into(),
            "".into(),
            "bash".into(),
            "ms".into(),
        );
        assert_eq!(row_label(&k2), "shell.startup (bash) (ms)");
    }

    #[test]
    fn best_cell_direction() {
        let lower = serde_json::json!({"median": 1.0});
        let higher = serde_json::json!({"median": 2.0});
        // lower-is-better metric: smaller median wins.
        assert!(is_better("shell.startup", &lower, &higher));
        assert!(!is_better("shell.startup", &higher, &lower));
        // higher-is-better fallback: render.throughput.
        assert!(is_better("render.throughput", &higher, &lower));
        // measurement.direction overrides the fallback.
        let hi_dir =
            serde_json::json!({"median": 2.0, "measurement": {"direction": "higher-is-better"}});
        let lo_dir =
            serde_json::json!({"median": 1.0, "measurement": {"direction": "higher-is-better"}});
        assert!(is_better("shell.startup", &hi_dir, &lo_dir));
    }

    #[test]
    fn competition_ranking_respects_direction_ties_and_missing_cells() {
        let lower_cells = BTreeMap::from([
            (
                0,
                serde_json::json!({
                    "median": 1.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
            (
                1,
                serde_json::json!({
                    "median": 1.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
            (
                3,
                serde_json::json!({
                    "median": 2.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
        ]);
        assert_eq!(
            competition_ranks("shell.startup", &lower_cells),
            BTreeMap::from([(0, 1), (1, 1), (3, 3)])
        );
        assert!(!competition_ranks("shell.startup", &lower_cells).contains_key(&2));

        let higher_cells = BTreeMap::from([
            (
                0,
                serde_json::json!({
                    "median": 2.0,
                    "measurement": {"direction": "higher-is-better"}
                }),
            ),
            (
                1,
                serde_json::json!({
                    "median": 3.0,
                    "measurement": {"direction": "higher-is-better"}
                }),
            ),
            (
                2,
                serde_json::json!({
                    "median": 3.0,
                    "measurement": {"direction": "higher-is-better"}
                }),
            ),
        ]);
        assert_eq!(
            competition_ranks("custom.metric", &higher_cells),
            BTreeMap::from([(0, 3), (1, 1), (2, 1)])
        );
    }

    #[test]
    fn html_views_show_competition_ranks_without_ranking_missing_cells() {
        let key: Key = (
            "shell.startup".into(),
            String::new(),
            "bash".into(),
            "ms".into(),
        );
        let cells = BTreeMap::from([
            (
                0,
                serde_json::json!({
                    "median": 1.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
            (
                1,
                serde_json::json!({
                    "median": 1.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
            (
                3,
                serde_json::json!({
                    "median": 2.0,
                    "measurement": {"direction": "lower-is-better"}
                }),
            ),
        ]);
        let labels = vec!["A".into(), "B".into(), "missing".into(), "D".into()];
        let rows = HashMap::from([(key.clone(), cells.clone())]);

        let glance = build_glance_table(std::slice::from_ref(&key), &rows, &labels);
        assert_eq!(glance.matches("Rank 1 of 3").count(), 2);
        assert_eq!(glance.matches("Rank 3 of 3").count(), 1);
        assert!(!glance.contains("Rank 2 of 3"));
        assert_eq!(glance.matches("aria-label=\"No result\"").count(), 1);

        let chart = build_bar_chart_svg(&key, &cells, &labels);
        assert_eq!(chart.matches("data-rank=\"1 of 3\"").count(), 2);
        assert_eq!(chart.matches("data-rank=\"3 of 3\"").count(), 1);
        assert!(chart.contains("rank 3 of 3, median 2.00 ms"));
        assert!(chart.contains("#3 \u{00b7} 2.00 ms"));
    }

    #[test]
    fn full_html_report_wires_rank_contract_into_table_tooltip_and_explanation() {
        let reports = vec![
            serde_json::json!({
                "results": [{
                    "metric": "shell.startup",
                    "shell": "bash",
                    "unit": "ms",
                    "median": 1.0,
                    "measurement": {"direction": "lower-is-better"}
                }]
            }),
            serde_json::json!({
                "results": [{
                    "metric": "shell.startup",
                    "shell": "bash",
                    "unit": "ms",
                    "median": 2.0,
                    "measurement": {"direction": "lower-is-better"}
                }]
            }),
        ];
        let labels = vec!["A".into(), "B".into()];
        let files = vec!["a.json".into(), "b.json".into()];

        let html = build_html_report(&reports, &labels, &files, 0);

        assert!(html.contains("<h2>Table view</h2>"));
        assert!(html.contains("Rank 1 of 2 \u{00b7} 1.00 / 1.00 / 0.00000"));
        assert!(html.contains("Rank 2 of 2 \u{00b7} 2.00 / 2.00 / 0.00000"));
        assert!(html.contains("data-rank=\"1 of 2\""));
        assert!(html.contains("rows.push(['Rank', d.rank]);"));
        assert!(html
            .contains("#N / Rank N = direction-aware competition rank (exact ties share a rank)"));
        assert!(html
            .contains("Cells: direction-aware competition rank \u{00b7} median / p95 / stddev."));
    }

    #[test]
    fn threshold_parsing() {
        assert_eq!(parse_threshold("10%"), Some(0.10));
        assert_eq!(parse_threshold("2.5%"), Some(0.025));
        assert_eq!(parse_threshold(" 5 "), Some(0.05));
        assert_eq!(parse_threshold("bad"), None);
        assert_eq!(parse_threshold("-5%"), None);
        assert_eq!(parse_threshold("NaN"), None);
        assert_eq!(parse_threshold("inf"), None);
        assert_eq!(parse_threshold("0"), Some(0.0));
    }

    #[test]
    fn regression_gate_fails_when_a_baseline_row_is_missing() {
        let row = |metric: &str, median: f64| {
            serde_json::json!({
                "metric": metric, "shell": "zsh", "unit": "ms", "median": median,
                "measurement": {"direction": "lower-is-better"}
            })
        };
        let baseline = serde_json::json!({
            "results": [row("shell.startup", 10.0), row("cmd.overhead", 5.0)]
        });
        let same = baseline.clone();
        let dropped = serde_json::json!({"results": [row("shell.startup", 10.0)]});
        let labels = vec!["a".to_string(), "b".to_string()];

        let (_, code) = build_regression_report(&[baseline.clone(), same], &labels, 0.1);
        assert_eq!(code, 0);
        let (text, code) = build_regression_report(&[baseline, dropped], &labels, 0.1);
        assert_eq!(code, 3);
        assert!(text.contains("- cmd.overhead (zsh) (ms)"), "{text}");
    }

    #[test]
    fn regression_uses_registered_direction() {
        let baseline = serde_json::json!({
            "median": 100.0,
            "measurement": {"direction": "higher-is-better"}
        });
        let slower = serde_json::json!({"median": 80.0});
        let faster = serde_json::json!({"median": 120.0});
        assert_eq!(
            regression_fraction("fire.fps", &baseline, &slower),
            Some(0.2)
        );
        assert_eq!(
            regression_fraction("fire.fps", &baseline, &faster),
            Some(-0.2)
        );
    }
}

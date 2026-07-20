# Benchmark Metrics in Detail

What each benchmark (metric) measures, how it measures it, and how far to
trust the numbers — units, variants, backends, and known limitations. This
is the long-form companion to the one-line summaries in the README's
Metrics table. The definitions live in `scenarios/*.yaml` and
`src/runner.rs` / `src/measure.rs` / `src/stats.rs` / `src/resource.rs`.

Common behavior:

- Each metric runs N trials (default 20, `--runs`) and reports **median /
  p95 / stddev**. Exceptions: `resource.*` (up to 5 point-in-time samples
  per variant) and `fire.fps` (duration-based; trial count is
  `min(--runs, 5)`).
- "Direction": low = lower is better, high = higher is better. In the JSON,
  `measurement.direction` is the authoritative source.
- Process-timed and PTY-echo trials get a warmup (default 3 discarded
  runs) and 1.5×IQR outlier filtering. The removed count is recorded per
  result as `outliers_removed`. `resource.*` point samples and duration-
  based `fire.fps` trials do not use this filter.
- A `--settle` pause (default 2s) separates scenarios and variants so one
  measurement's leftover load doesn't pollute the next one's first samples.
- A measured child process that does not exit within 30 seconds is killed
  and the trial is rejected; non-zero exits are rejected as well.

## Overview

| Metric | Unit | Direction | Shell-scoped? | Variants | Measures (one line) |
|---|---|---|---|---|---|
| `shell.startup` | ms | low | yes | — | Fresh interactive shell process startup time |
| `cmd.overhead` | μs | low | yes | — | Shell process spawn + builtin dispatch cost |
| `render.throughput` | MB/s, lines/s | high | no | `plain` / `ansi` / `cjk` | Producer-side PTY output throughput |
| `scroll.proxy` | ms | low | no | — | Time until the large-output producer exits (proxy) |
| `input.latency` | ms | low | no | `idle` / `loaded` | PTY echo round-trip latency |
| `resource.mem` | MB | low | no | `idle` / `load` | RSS of the emulator's GUI process |
| `resource.cpu` | % | low | no | `idle` / `load` | %CPU of the emulator's GUI process |
| `fire.fps` | fps | high | no | `<columns>x<rows>` | Producer-completed frames/s for a DOOM-fire repaint stream |

Shell-scoped = yes means the metric runs once per entry in `--shells`
(default `zsh,bash`) and reports that shell in `"shell"`. Shell-scoped = no
means it measures the terminal emulator itself, runs once regardless of
`--shells`, and reports `"shell": null`.

---

## shell.startup — interactive shell startup time

- **What it measures**: wall-clock time of `{shell} -i -c exit`. `-i`
  (interactive) means rc files (`.zshrc` / `.bashrc` etc.) **are** loaded —
  this is the metric for "how much did my dotfiles / plugin-manager change
  slow down startup".
- **How**: a built-in monotonic timer (`std::time::Instant`) around a
  direct process spawn — no shell, no fork per sample. Recorded per result
  as backend `"rust-timer"`. (Reports from the retired `hyperfine`/
  `zsh-timer` backends remain valid; differing backends are
  NOT_COMPARABLE under the pairwise gate.)
- **Unit / direction**: ms, lower is better.
- **Caveats**: supports `--runs auto` (adaptive sampling). Cross-machine
  absolute comparisons still require matching environments — see the
  Reporting Spec's pairwise gate. The run-context shell fact records the
  `--version` output for every measured shell in deterministic name order;
  each result's shell dimension identifies which one produced that row.
  Each trial starts a new process, but filesystem/page caches are not
  cleared, so this is not a machine-cold boot measurement.

## cmd.overhead — command dispatch overhead

- **What it measures**: wall-clock time of `{shell} -c true` = shell
  process spawn + dispatch of the `true` builtin + exit. This is the fixed
  per-invocation cost a script pays every time it launches a shell.
- **How**: same as `shell.startup` (`rust-timer`). No rc files are loaded
  (non-interactive), so the difference from `shell.startup` is
  approximately the rc-file contribution.
- **Unit / direction**: μs, lower is better.
- **Caveats**: fork/exec cost has high relative variance; under
  `--runs auto` this metric often hits the 50-trial cap instead of
  converging early (expected, not a failure).

## render.throughput — draw throughput

- **What it measures**: `cat`s a fixed-size fixture file straight to the
  terminal and derives **MB/s** and **lines/s** (two rows) from the
  wall-clock time. It measures the terminal emulator tbench is running
  inside — more precisely its PTY drain rate — not shell performance.
- **Variants**:
  - `plain` — ASCII-only log-like text
  - `ansi` — with ANSI color sequences (adds escape-parsing cost)
  - `cjk` — mixed full-width Japanese text and emoji. UTF-8 encodes those
    characters as 3-4 bytes each, so **`cjk`'s MB/s is not byte-for-byte
    comparable** to `plain`/`ansi`; use lines/s to compare across variants.
  - Fixtures are generated lazily on first use (built into the binary;
    same content as the retired `fixtures/gen-fixtures.sh`).
- **Unit / direction**: MB/s and lines/s, higher is better.
- **Caveats**: this relies on `write()` blocking on PTY flow control — it
  measures a **drain rate**, not confirmed on-screen rendering (an emulator
  that skips frames can look faster). The wall-clock boundary also includes
  `cat` process creation and fixture file reads, so executable and page-cache
  state are part of the result. Whenever
  `meta.synchronized_output_supported` isn't `"supported"`,
  `meta.frame_skip_risk_note` flags this. Without a controlling terminal
  (`/dev/tty`, e.g. CI), the metric is skipped; it is never relabelled raw
  `/dev/null` I/O.

## scroll.proxy — scroll performance (proxy)

- **What it measures**: total wall-clock time for a large `cat` producer to
  finish writing to the controlling terminal.
- **How**: no native scroll/redraw-event API exists, so "time until output
  completes" is used as a **proxy indicator** (proposal §3 item 3).
- **Unit / direction**: ms, lower is better.
- **Caveats**: a coarse proxy that does not observe scrollback settling or
  any redraw event. It also includes `cat` process creation and fixture file
  reads. Like `render.throughput`, it is skipped without a controlling terminal.

## input.latency — input latency (PTY round trip)

- **What it measures**: spawns `cat` on a fresh pseudo-terminal
  (`openpty`), writes one byte to the master side, and times until the
  echoed byte is read back. Backend: `"rust-pty"` (the retired zsh
  implementation reported `"zsh-zpty"`).
- **Variants**:
  - `idle` — the round trip with nothing else running
  - `loaded` — the same measurement while a `render.throughput`-style flood
    (the `plain` fixture) streams to the terminal in the background, to see
    how much output contention degrades latency and especially its
    **jitter (p95)**.
- **Unit / direction**: ms, lower is better.
- **Caveats**: this is a pseudo-terminal echo round trip, **not real
  keypress-to-screen-photon latency** (that needs GUI automation — out of
  scope, proposal §6.6 extension 3). If no pty is available
  (sandboxes/CI), the variant is skipped with a warning. A timeout or
  missing echo invalidates the whole variant instead of being silently
  dropped. The observed echo can be provided by the PTY line discipline,
  so this does not isolate `cat` processing. Does not support `--runs auto`
  (falls back to a fixed trial count).

## resource.mem / resource.cpu — resource footprint

- **What it measures**: RSS (MB) and %CPU of the **terminal emulator's own
  GUI process** — not the shell, not tbench. The process is found by
  walking the process ancestry from tbench upward to the first executable
  inside a macOS `.app` bundle (`.../Contents/MacOS/<bin>`).
- **Variants**:
  - `idle` — at rest. macOS's `ps %cpu` is a decayed running average, so
    idle sampling waits **at least 5s** regardless of `--settle` to avoid
    inheriting the previous scenario's load.
  - `load` — sampled while a `render.throughput`-style flood is running.
    If that flood cannot start, the load rows are skipped rather than
    reported as unloaded samples.
- **How**: up to 5 point-in-time `ps` samples per variant (fewer if
  `--runs` < 5); the median is reported.
- **Unit / direction**: MB / %, both lower is better.
- **Caveats**: inside tmux, over SSH, or on non-macOS the target process
  can't be identified and the scenario is skipped with a warning. Because
  of macOS's decayed-average `%cpu` semantics, treat `resource.cpu` as
  **directional**, not an instantaneous reading.

## fire.fps — DOOM-fire render stress

- **What it measures**: runs the integrated DOOM-fire workload from
  `src/fire.rs` (a faithful Rust port compatible with the retired vendored
  C implementation, formerly `tools/fire.c` — identical palette, seed,
  frame order, and byte stream), which repaints a truecolor
  half-block region every frame at the live window size (full-window
  condition), and reports **frames/second**.
- **How**: each trial is duration-based — it runs `FIRE_SECS` wall-clock
  seconds (env var, default 5; finite range 0.1–60) rather than a fixed iteration count — and
  the trial count is capped at `min(--runs, 5)` to bound total suite
  runtime. No C compiler is needed: the workload is compiled into the
  tbench binary.
- **Producer**: a writer thread drains frame N into the pty while the main
  thread composes frame N+1 (`fire-v2-overlapped`, recorded in the result's
  `backend` field). Throughput is limited by the slower of composition and
  PTY writing; it does not isolate drain rate —
  identical to the C tool, so their fps values are directly comparable.
  fps from the retired serial (v1) producer is **not** — check `backend`
  before comparing.
- **Unit / direction**: fps, higher is better.
- **Caveats**: like `render.throughput`, this is a **producer-side
  PTY-drain-rate proxy**, not a confirmed on-screen frame rate. fps scales
  roughly with 1/cell-count. The rendered `<columns>x<rows>` region is the
  result variant and workload identity, so differing geometries do not
  compare as the same result. Without a controlling tty the metric is
  skipped.

---

## Where summaries appear in the output

`bin/tbench show` (and `run`'s post-run recap) and `bin/tbench compare`'s
Markdown and HTML outputs append a one-line-per-metric "Metric guide" for
the metrics present in the report. This document is the full-detail
counterpart to those one-liners.

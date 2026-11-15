# A/A baseline for the regression gate

Reference numbers for `tbench compare --threshold`: how often the gate
flags two runs of the same configuration as different. Any change to the
gate's statistics should be measured against this.

## Method

- Host: Linux x86_64 (kernel 7.2), kitty, no tty-bound scenarios involved.
- 12 back-to-back runs each of `cmd-overhead` and `shell-startup`:

  ```sh
  for i in $(seq -w 1 12); do
    for sc in cmd-overhead shell-startup; do
      tbench run --scenario $sc --settle 0 --keep-samples --app aa \
        --output aa/$sc-$i.json
    done
  done
  python3 tools/aa_baseline.py aa
  ```

- Defaults otherwise: 20 trials, 3 warmup trials, 1.5×IQR filtering.
- `tools/aa_baseline.py` reimplements the gate's Mann-Whitney U test
  (two-sided, normal approximation, tie-corrected) and counts, per row,
  the unordered pairs with p<0.05 (66 per row) and the ordered
  baseline/latest pairs that fail the gate at each threshold (132 per row).

## Result (2026-11-15, tbench 2.0.0-dev at 31aaf8b)

| Metric | Median range | Spread | Pairs with p<0.05 (of 66) | Ordered pairs failing at 0% / 2% / 5% / 10% (of 132) |
|---|---|---|---|---|
| `cmd.overhead` (zsh) | 3633.96 to 3734.72 μs | 2.8% | 17 (25.8%) | 17 / 7 / 0 / 0 |
| `cmd.overhead` (bash) | 994.19 to 1054.18 μs | 6.0% | 19 (28.8%) | 19 / 16 / 7 / 0 |
| `shell.startup` (zsh) | 7.09 to 7.48 ms | 5.5% | 32 (48.5%) | 32 / 26 / 2 / 0 |
| `shell.startup` (bash) | 24.72 to 25.00 ms | 1.1% | 2 (3.0%) | 2 / 0 / 0 / 0 |

Overall 70 of 264 identical pairs (26.5%) test as significantly different,
against a nominal 5%: about 5.3 times too many.

## Reading it

- The test treats the trials inside one run as independent draws and has
  no term for run-to-run variation. Medians of identical runs drifted by 1
  to 6%, which is enough to separate two tight within-run distributions.
- At a 10% threshold nothing failed here. False failures start at 5% and
  below.
- One quiet machine, 12 runs, and pairs that share runs: a baseline to
  beat, not a precise rate. Re-run it on the machine where the gate is used.

## After the run-level gate

The gate now compares the median of per-run medians and, with at least 4
runs on each side, tests significance across runs instead of across the
trials inside one run. Same 24 report files, same binary logic as shipped:

| Data | Before: 1 run vs 1, trial-level test | After: 4 runs vs 4, run-level test |
|---|---|---|
| `cmd-overhead`, threshold 0% | 25 of 132 pairs fail (18.9%) | 6 of 250 random splits (2.4%) |
| `cmd-overhead`, threshold 2% | 18 of 132 (13.6%) | 2 of 250 (0.8%) |
| `cmd-overhead`, threshold 5% | 7 of 132 (5.3%) | 0 of 250 |
| `shell-startup`, threshold 0% | 34 of 132 (25.8%) | 4 of 250 (1.6%) |
| `shell-startup`, threshold 2% | 26 of 132 (19.7%) | 3 of 250 (1.2%) |
| `shell-startup`, threshold 5% | 2 of 132 (1.5%) | 0 of 250 |
| `resource-footprint` (8 runs through a pty relay), threshold 2% | 23 of 56 (41.1%) | 1 of 70 splits (1.4%) |

With one run on each side there is no significance test any more; the
threshold alone decides. On this data that fails 8 of 132 `cmd-overhead`
pairs and 2 of 132 `shell-startup` pairs at 5%.

Detection, from a simulation that scales the latest runs of the same data
(4 runs vs 4): a true +5% change is caught 84% of the time at a 2%
threshold and 46% at a 5% threshold; +10% is caught every time at 2% and 5%
and about half the time at 10%; +20% is always caught.

The splits reuse 12 back-to-back runs from one session. Drift between
sessions (thermal state, background load, a reboot) is not in this data and
will make real false failures more frequent than shown.

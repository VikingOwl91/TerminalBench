#!/bin/zsh
# report-v1.sh — Reporting Specification v1 regression suite, driven
# against the Rust single-binary implementation (bin/tbench launcher +
# bin/tbench-validate validator CLI). python3 is used only as a test-side
# JSON assertion tool; the implementation under test has no runtime
# dependency on it.
set -eu
set -o pipefail

ROOT="${0:A:h:h}"
VALID="$ROOT/fixtures/conformance/v1/valid-minimal.json"
INVALID="$ROOT/fixtures/conformance/v1/invalid-proxy-overclaim.json"
VALIDATOR="${TBENCH_VALIDATOR:-$ROOT/bin/tbench-validate}"
TBENCH="${TBENCH_BIN:-$ROOT/bin/tbench}"
PORTABLE="${TBENCH_PORTABLE:-0}"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/tbench-report-v1.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

pass_count=0
fail() { print -u2 -- "report-v1: FAIL: $*"; exit 1 }
pass() { pass_count=$((pass_count + 1)); print -- "ok $pass_count - $*"; }

expect_status() {
  python3 - "$1" "$2" <<'PY'
import json, sys
with open(sys.argv[2], encoding="utf-8") as handle:
    actual = json.load(handle).get("status")
assert actual == sys.argv[1], (actual, sys.argv[1])
PY
}

expect_failure() {
  local expected_code="$1" expected_rule="$2" expected_pointer="$3" file="$4"
  set +e
  "$VALIDATOR" validate "$file" >"$TMP/out" 2>"$TMP/err"
  local code=$?
  set -e
  [[ $code -ne 0 ]] || fail "$file was accepted"
  if [[ "$PORTABLE" != 1 ]]; then
    [[ $code -eq $expected_code ]] || fail "$file exit $code, expected $expected_code"
    grep -Fq "\"rule_id\":\"$expected_rule\"" "$TMP/err" || fail "$file missing rule $expected_rule"
    grep -Fq "\"pointer\":\"$expected_pointer\"" "$TMP/err" || fail "$file missing pointer $expected_pointer"
  fi
}

[[ -x "$VALIDATOR" ]] || fail "validator is not executable: $VALIDATOR"

if [[ "$PORTABLE" != 1 ]]; then
python3 - "$ROOT/schemas/tbench-report-v1.schema.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as f:
    schema = json.load(f)
assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema"
assert schema["$id"] == "urn:tbench:schema:report:v1"
assert schema["additionalProperties"] is False
assert schema["$defs"]["measurement"]["additionalProperties"] is False
assert schema["properties"]["wire_version"] == {"const": "1.0.0"}
assert "structural subset" in schema["$comment"].lower()
assert schema["$defs"]["knownNumber"]["properties"]["value"]["maximum"] == 1e308
assert schema["$defs"]["osFact"]["oneOf"][0]["properties"]["value"]["pattern"].startswith("^(?:darwin")
assert schema["$defs"]["measurement"]["properties"]["profile_version"] == {"const": "1.0.0"}
assert schema["$defs"]["syncFact"]["oneOf"][0]["properties"]["value"]["enum"] == ["supported", "unsupported", "no_reply"]
assert "pattern" in schema["$defs"]["meta"]["properties"]["timestamp"]
expected_notes = {
    "render.throughput": "Proxy only: measures producer-side PTY write completion; it does not observe terminal parsing, final-frame presentation, frame time, FPS, pixels presented, or complete rendering.",
    "scroll.proxy": "Proxy only: measures producer-side PTY write completion for the scroll workload; it does not observe scroll, redraw, display events, frame time, FPS, pixels presented, or complete rendering.",
    "input.latency": "Proxy only: measures a synthetic PTY echo round trip; it does not observe physical keypress, GUI input latency, screen response, keypress-to-display, or keypress-to-photon latency.",
    "fire.fps": [
        "Proxy only: measures producer-completed frames for a truecolor repaint stream at the cell geometry recorded by variant; throughput includes overlapped frame composition and PTY writing and does not observe terminal parsing, compositing, or confirmed on-screen frame presentation, so this fps is not a measured render frame rate.",
        "Proxy only: measures producer-side PTY write completion for a live window-sized truecolor repaint stream; it does not observe terminal parsing, compositing, or confirmed on-screen frame presentation, so this fps is a producer-side drain-rate proxy, not a measured render frame rate, and it scales inversely with window cell count so values are only comparable at matching window geometry.",
    ],
}
conditions = schema["$defs"]["result"]["allOf"]
assert all("required" not in item["then"] for item in conditions)
schema_notes = {}
for item in conditions:
    metric = item["if"]["properties"]["metric"]["const"]
    notes_rule = item["then"]["properties"]["notes"]
    schema_notes[metric] = notes_rule.get("const", notes_rule.get("enum"))
assert schema_notes == expected_notes
PY
pass "canonical Draft 2020-12 structural schema closes wire, profiles, controls, and proxy notes"
fi

"$VALIDATOR" validate "$VALID" >"$TMP/valid.out"
if [[ "$PORTABLE" != 1 ]]; then
  expect_status CONFORMANT "$TMP/valid.out"
fi
pass "valid minimal fixture conforms"

expect_failure 4 TBV-CLAIM-BOUNDARY /results/0/measurement/classification "$INVALID"
if [[ "$PORTABLE" != 1 ]]; then
  grep -Fq '"pointer":"/results/0/measurement/observed_boundary"' "$TMP/err" || fail "proxy overclaim lacks boundary pointer"
fi
pass "proxy overclaim fails at stable claim-boundary pointers"

# Generate one runtime report containing every registered profile via the
# real emitter (hidden __smoke-report hook + deterministic env overrides),
# not a copied fixture.
TERM_PROGRAM="ConformanceTerm" TERM_PROGRAM_VERSION="1.0" \
TBENCH_TEST_SYNC_OUTPUT=supported TBENCH_TEST_DISABLE_MACOS_META=1 \
TBENCH_MACHINE_MODEL=ExampleMachine TBENCH_POWER_SOURCE=ac \
TBENCH_LOW_POWER_MODE=false TBENCH_DISPLAY_REFRESH_HZ=60 \
  "$TBENCH" __smoke-report >"$TMP/smoke.json"
"$VALIDATOR" validate "$TMP/smoke.json" >/dev/null
if [[ "$PORTABLE" != 1 ]]; then
python3 - "$TMP/smoke.json" "$ROOT/docs/reporting-spec-v1.md" "$ROOT/schemas/tbench-report-v1.schema.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
spec = open(sys.argv[2], encoding="utf-8").read()
notes = {r["metric"]: r.get("notes") for r in report["results"] if r["metric"] in {"render.throughput", "scroll.proxy", "input.latency", "fire.fps"}}
schema = json.load(open(sys.argv[3], encoding="utf-8"))
schema_notes = {}
for item in schema["$defs"]["result"]["allOf"]:
    metric = item["if"]["properties"]["metric"]["const"]
    notes_rule = item["then"]["properties"]["notes"]
    schema_notes[metric] = notes_rule.get("const", notes_rule.get("enum"))
assert "pixels presented" in notes["render.throughput"]
assert "keypress-to-display" in notes["input.latency"]
assert all(note.startswith("Proxy only:") for note in notes.values())
assert all(note in spec for note in notes.values())
assert all(
    notes[metric] in allowed if isinstance(allowed, list) else notes[metric] == allowed
    for metric, allowed in schema_notes.items()
)
PY
fi
pass "runtime-generated report validates across every known profile"

# Darwin with every optional metadata value absent: the legacy TERM stays
# descriptive, but v1 must not infer an emulator identity from it.
env -u TBENCH_TERMINAL_ID -u TBENCH_MACHINE_MODEL \
  -u TBENCH_POWER_SOURCE -u TBENCH_LOW_POWER_MODE \
  -u TBENCH_DISPLAY_REFRESH_HZ \
  -u TERM_PROGRAM -u TERM_PROGRAM_VERSION TERM=screen-256color \
  TBENCH_TEST_SYNC_OUTPUT=no_reply TBENCH_TEST_DISABLE_MACOS_META=1 \
  "$TBENCH" __smoke-report >"$TMP/empty-meta.json"
"$VALIDATOR" validate "$TMP/empty-meta.json" >/dev/null
if [[ "$PORTABLE" != 1 ]]; then
python3 - "$TMP/empty-meta.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
assert report["meta"]["terminal"] == "screen-256color"
assert report["run_context"]["environment"]["terminal"]["status"] == "unknown"
for key in ("power_source", "low_power_mode", "machine_model", "gpu", "display_refresh_hz"):
    assert key not in report["meta"]
PY
fi
pass "empty optional metadata stays conformant and TERM is not inferred as emulator identity"

for override_case in 'a|PortableTerm A|60' 'b|PortableTerm B|59.94'; do
  report_name="${override_case%%|*}"
  rest="${override_case#*|}"
  terminal_id="${rest%%|*}"
  refresh="${rest##*|}"
  TBENCH_TERMINAL_ID="$terminal_id" \
  TBENCH_MACHINE_MODEL="CI-Runner-A" \
  TBENCH_POWER_SOURCE=ac \
  TBENCH_LOW_POWER_MODE=false \
  TBENCH_DISPLAY_REFRESH_HZ="$refresh" \
  TBENCH_TEST_SYNC_OUTPUT=supported TBENCH_TEST_DISABLE_MACOS_META=1 \
    "$TBENCH" __smoke-report >"$TMP/override-$report_name.json"
  "$VALIDATOR" validate "$TMP/override-$report_name.json" >/dev/null
done
"$VALIDATOR" compare "$TMP/override-a.json" "$TMP/override-b.json" >"$TMP/override-compare.out"
expect_status COMPARABLE "$TMP/override-compare.out"
if [[ "$PORTABLE" != 1 ]]; then
python3 - "$TMP/override-a.json" "$TMP/override-b.json" <<'PY'
import json, sys
for path in sys.argv[1:]:
    report = json.load(open(path, encoding="utf-8"))
    env, meta = report["run_context"]["environment"], report["meta"]
    for fact, legacy in (("terminal", "terminal"), ("machine_model", "machine_model"), ("power_source", "power_source"), ("low_power_mode", "low_power_mode"), ("display_refresh_hz", "display_refresh_hz")):
        assert env[fact] == {"status": "known", "value": meta[legacy]}, (path, fact, env[fact], meta.get(legacy))
    assert env["architecture"]["value"] in {"aarch64", "x86_64"} or env["architecture"]["value"].startswith("other:")
PY
fi

TBENCH_TERMINAL_ID=' ' TBENCH_MACHINE_MODEL=' bad ' TBENCH_POWER_SOURCE=mains \
TBENCH_LOW_POWER_MODE=0 TBENCH_DISPLAY_REFRESH_HZ=060 \
TBENCH_TEST_SYNC_OUTPUT=supported TBENCH_TEST_DISABLE_MACOS_META=1 \
  "$TBENCH" __smoke-report >"$TMP/override-invalid.json"
"$VALIDATOR" validate "$TMP/override-invalid.json" >/dev/null
if [[ "$PORTABLE" != 1 ]]; then
python3 - "$TMP/override-invalid.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
env, meta = report["run_context"]["environment"], report["meta"]
for fact in ("terminal", "machine_model", "power_source", "low_power_mode", "display_refresh_hz"):
    assert env[fact]["status"] == "unknown"
for name in ("TBENCH_TERMINAL_ID", "TBENCH_MACHINE_MODEL", "TBENCH_POWER_SOURCE", "TBENCH_LOW_POWER_MODE", "TBENCH_DISPLAY_REFRESH_HZ"):
    assert name in meta["meta_notes"]
PY
fi
pass "portable overrides preserve fact/meta parity, comparability, and fail-soft invalid values"

mkdir -p "$TMP/zdotdir"
: >"$TMP/zdotdir/.zshrc"
if ! TERM_PROGRAM="ConformanceTerm" TERM_PROGRAM_VERSION="1.0" \
  ZDOTDIR="$TMP/zdotdir" \
  "$TBENCH" run --scenario shell-startup --shells /bin/zsh --runs 1 \
  --settle 0 --warmup 0 --force --output "$TMP/actual.json" \
  >"$TMP/actual.out" 2>"$TMP/actual.err"; then
  cat "$TMP/actual.err" >&2
  fail "actual one-trial tbench run failed"
fi
"$VALIDATOR" validate "$TMP/actual.json" >/dev/null
pass "actual one-trial tbench run emits a conformant report"

if ! TERM_PROGRAM="ConformanceTerm" TERM_PROGRAM_VERSION="1.0" \
  TBENCH_TERMINAL_ID="EnvTerm 1.0" ZDOTDIR="$TMP/zdotdir" \
  "$TBENCH" run --scenario shell-startup --shells /bin/zsh --runs 1 \
  --settle 0 --warmup 0 --force --app "CliTerm 2.0" \
  --output "$TMP/actual-app.json" \
  >"$TMP/actual-app.out" 2>"$TMP/actual-app.err"; then
  cat "$TMP/actual-app.err" >&2
  fail "tbench run --app failed"
fi
"$VALIDATOR" validate "$TMP/actual-app.json" >/dev/null
if [[ "$PORTABLE" != 1 ]]; then
python3 - "$TMP/actual-app.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
assert report["meta"]["terminal"] == "CliTerm 2.0"
assert report["run_context"]["environment"]["terminal"] == {"status": "known", "value": "CliTerm 2.0"}
PY
fi
if "$TBENCH" run --app ' bad ' --force >/dev/null 2>/dev/null; then
  fail "run --app accepted an untrimmed identifier"
fi
pass "run --app sets the terminal identity, beats TBENCH_TERMINAL_ID, and rejects invalid values"

if [[ "$PORTABLE" != 1 ]]; then
  "$VALIDATOR" profiles >"$TMP/profiles.json"
  python3 - "$ROOT/schemas/tbench-report-v1.schema.json" "$TMP/smoke.json" "$TMP/profiles.json" <<'PY'
import json, sys
schema, report, profiles = (json.load(open(p, encoding="utf-8")) for p in sys.argv[1:])
emitted = {r["measurement"]["profile_id"] for r in report["results"]}
assert emitted == set(profiles)
required = set(schema["$defs"]["measurement"]["required"])
assert all(set(r["measurement"]) == required for r in report["results"])
schema_metrics = set(schema["$defs"]["result"]["properties"]["metric"]["enum"])
assert schema_metrics == {r["metric"] for r in report["results"]}
PY
else
  : # Portable mode checks semantic acceptance, not reference registry internals.
fi
if [[ "$PORTABLE" != 1 ]]; then
  pass "schema, validator registry, emitter profiles, and required fields are in parity"
fi

python3 - "$VALID" "$TMP/smoke.json" "$TMP" <<'PY'
import copy, json, sys
base = json.load(open(sys.argv[1], encoding="utf-8"))
smoke = json.load(open(sys.argv[2], encoding="utf-8"))
out = sys.argv[3]
cases = {}
def case(name, fn):
    value = copy.deepcopy(base); fn(value); cases[name] = value
case("wire-major", lambda x: x.__setitem__("wire_version", "2.0.0"))
case("wire-minor", lambda x: x.__setitem__("wire_version", "1.1.0"))
case("wire-patch", lambda x: x.__setitem__("wire_version", "1.0.1"))
case("wire-leading-zero", lambda x: x.__setitem__("wire_version", "1.01.0"))
case("profile-version", lambda x: x["results"][0]["measurement"].__setitem__("profile_version", "2.0.0"))
case("unknown-profile", lambda x: x["results"][0]["measurement"].__setitem__("profile_id", "org.terminalbench.future"))
case("unknown-core", lambda x: x.__setitem__("vendor", {}))
case("missing-field", lambda x: x["results"][0]["measurement"].pop("limitations"))
case("negative", lambda x: x["results"][0].__setitem__("median", -1))
case("nonfinite", lambda x: x["results"][0].__setitem__("median", float("nan")))
case("p95", lambda x: x["results"][0].__setitem__("p95", 1))
case("runs", lambda x: x["results"][0].__setitem__("runs", 0))
case("outliers", lambda x: x["results"][0].__setitem__("outliers_removed", 4))
case("samples", lambda x: x["results"][0].__setitem__("samples", [10]))
case("empty", lambda x: x.__setitem__("results", []))
case("duplicate", lambda x: x["results"].append(copy.deepcopy(x["results"][0])))
case("unit-profile", lambda x: x["results"][0]["measurement"].__setitem__("unit", "us"))
case("method", lambda x: x["results"][0]["measurement"].__setitem__("method_id", "org.terminalbench.method.process-sample"))
case("boundary", lambda x: x["results"][0]["measurement"].__setitem__("observed_boundary", "screen-response"))
def proxy_note_case(name, metric, text):
    value = copy.deepcopy(smoke)
    result = next(r for r in value["results"] if r["metric"] == metric)
    value["results"] = [result]
    result["notes"] = text
    cases[name] = value
proxy_note_case("render-pixels", "render.throughput", "Producer reports pixels presented at this rate.")
proxy_note_case("input-display", "input.latency", "Measures keypress-to-display latency.")
value = copy.deepcopy(smoke)
value["results"] = [next(r for r in value["results"] if r["metric"] == "scroll.proxy")]
value["results"][0].pop("notes")
cases["proxy-note-absent"] = value
case("dimension", lambda x: x["results"][0]["measurement"]["dimensions"].__setitem__("shell", "bash"))
case("workload", lambda x: x["results"][0]["measurement"]["workload"].__setitem__("id", "urn:tbench:workload:other:v1"))
def unknown_workload(x):
    x["results"][0]["measurement"]["workload"] = {"status": "unknown", "reason": "producer did not retain workload identity"}
case("workload-unknown", unknown_workload)
case("bad-extension", lambda x: x.__setitem__("extensions", {"VendorData": {}}))
case("timestamp-syntax", lambda x: x["meta"].__setitem__("timestamp", "2026-07-16 00:00:00"))
case("timestamp-calendar", lambda x: x["meta"].__setitem__("timestamp", "2026-02-30T00:00:00Z"))
for field in ("frame_skip_risk_note", "power_source", "machine_model", "cpu", "gpu", "meta_notes"):
    case(f"meta-string-{field}", lambda x, field=field: x["meta"].__setitem__(field, 42))
case("meta-bool", lambda x: x["meta"].__setitem__("low_power_mode", "false"))
case("meta-number", lambda x: x["meta"].__setitem__("display_refresh_hz", 0))
case("env-sync", lambda x: x["run_context"]["environment"]["synchronized_output_supported"].__setitem__("value", "maybe"))
case("meta-sync", lambda x: x["meta"].__setitem__("synchronized_output_supported", "maybe"))
case("os-noncanonical", lambda x: x["run_context"]["environment"]["os"].__setitem__("value", "Darwin"))
case("arch-noncanonical", lambda x: x["run_context"]["environment"]["architecture"].__setitem__("value", "arm64"))
case("power-noncanonical", lambda x: x["run_context"]["environment"]["power_source"].__setitem__("value", "AC"))
case("machine-untrimmed", lambda x: x["run_context"]["environment"]["machine_model"].__setitem__("value", " ExampleMachine "))
case("terminal-parity", lambda x: x["run_context"]["environment"]["terminal"].__setitem__("value", "OtherTerm 2.0"))
case("meta-refresh-near", lambda x: x["meta"].__setitem__("display_refresh_hz", 59.94))
case("meta-refresh-far", lambda x: x["meta"].__setitem__("display_refresh_hz", 59.8))
def not_comparable(x):
    x["run_context"]["environment"]["os"]["value"] = "linux"
    x["meta"]["os"] = "linux"
case("not-comparable", not_comparable)
def other_terminal(x):
    x["run_context"]["environment"]["terminal"]["value"] = "OtherTerm 2.0"
    x["meta"]["terminal"] = "OtherTerm 2.0"
case("other-terminal", other_terminal)
def make_unknown(x):
    x["run_context"]["environment"]["terminal"] = {"status": "unknown", "reason": "not recorded"}
case("undetermined", make_unknown)
for name, refresh in (("refresh-near", 59.94), ("refresh-far", 59.8)):
    value = copy.deepcopy(smoke)
    value["run_context"]["environment"]["display_refresh_hz"]["value"] = refresh
    value["meta"]["display_refresh_hz"] = refresh
    cases[name] = value
for name, value in cases.items():
    with open(f"{out}/{name}.json", "w", encoding="utf-8") as f:
        json.dump(value, f)
legacy = copy.deepcopy(base)
for key in ("$schema", "wire_version", "producer", "protocol", "run_context"):
    legacy.pop(key)
for result in legacy["results"]:
    result.pop("measurement")
with open(f"{out}/legacy.json", "w", encoding="utf-8") as f:
    json.dump(legacy, f)
PY

python3 - "$VALID" "$TMP" <<'PY'
from pathlib import Path
import sys
source = Path(sys.argv[1]).read_text(encoding="utf-8")
out = Path(sys.argv[2])
replacements = {
    "duplicate-classification.json": ('        "classification": "sampled",', '        "classification": "proxy",\n        "classification": "sampled",'),
    "duplicate-terminal.json": ('      "terminal": {"status": "known", "value": "ExampleTerm 1.0"},', '      "terminal": {"status": "unknown", "reason": "shadow"},\n      "terminal": {"status": "known", "value": "ExampleTerm 1.0"},'),
    "duplicate-median.json": ('      "median": 10.0,', '      "median": 999.0,\n      "median": 10.0,'),
    "huge-median.json": ('      "median": 10.0,', '      "median": ' + ('9' * 1000) + ','),
}
for name, (needle, replacement) in replacements.items():
    assert source.count(needle) == 1
    (out / name).write_text(source.replace(needle, replacement), encoding="utf-8")
PY

expect_failure 5 TBV-WIRE-MAJOR /wire_version "$TMP/wire-major.json"
expect_failure 5 TBV-WIRE-MAJOR /wire_version "$TMP/wire-minor.json"
expect_failure 5 TBV-WIRE-MAJOR /wire_version "$TMP/wire-patch.json"
expect_failure 5 TBV-WIRE-MAJOR /wire_version "$TMP/wire-leading-zero.json"
expect_failure 6 TBV-PROFILE-VERSION /results/0/measurement/profile_version "$TMP/profile-version.json"
expect_failure 6 TBV-PROFILE-UNSUPPORTED /results/0/measurement/profile_id "$TMP/unknown-profile.json"
expect_failure 4 TBV-STRUCT-UNKNOWN /vendor "$TMP/unknown-core.json"
expect_failure 4 TBV-STRUCT-REQUIRED /results/0/measurement/limitations "$TMP/missing-field.json"
expect_failure 4 TBV-NUMBER-NONNEGATIVE /results/0/median "$TMP/negative.json"
expect_failure 3 TBV-IO-JSON '' "$TMP/nonfinite.json"
expect_failure 4 TBV-STAT-P95 /results/0/p95 "$TMP/p95.json"
expect_failure 4 TBV-RUNS /results/0/runs "$TMP/runs.json"
expect_failure 4 TBV-OUTLIERS /results/0/outliers_removed "$TMP/outliers.json"
expect_failure 4 TBV-SAMPLE-COUNT /results/0/samples "$TMP/samples.json"
expect_failure 4 TBV-RESULTS-NONEMPTY /results "$TMP/empty.json"
expect_failure 4 TBV-RESULT-DUPLICATE /results/1 "$TMP/duplicate.json"
expect_failure 4 TBV-PROFILE-MISMATCH /results/0/measurement/unit "$TMP/unit-profile.json"
expect_failure 4 TBV-PROFILE-MISMATCH /results/0/measurement/method_id "$TMP/method.json"
expect_failure 4 TBV-CLAIM-BOUNDARY /results/0/measurement/observed_boundary "$TMP/boundary.json"
expect_failure 4 TBV-CLAIM-NOTE /results/0/notes "$TMP/render-pixels.json"
expect_failure 4 TBV-CLAIM-NOTE /results/0/notes "$TMP/input-display.json"
expect_failure 4 TBV-DIMENSION-MISMATCH /results/0/measurement/dimensions/shell "$TMP/dimension.json"
expect_failure 4 TBV-WORKLOAD-MISMATCH /results/0/measurement/workload "$TMP/workload.json"
expect_failure 4 TBV-EXTENSION-NAMESPACE /extensions/VendorData "$TMP/bad-extension.json"
expect_failure 4 TBV-META-TIMESTAMP /meta/timestamp "$TMP/timestamp-syntax.json"
expect_failure 4 TBV-META-TIMESTAMP /meta/timestamp "$TMP/timestamp-calendar.json"
for field in frame_skip_risk_note power_source machine_model cpu gpu meta_notes; do
  expect_failure 4 TBV-META-TYPE "/meta/$field" "$TMP/meta-string-$field.json"
done
expect_failure 4 TBV-META-TYPE /meta/low_power_mode "$TMP/meta-bool.json"
expect_failure 4 TBV-META-TYPE /meta/display_refresh_hz "$TMP/meta-number.json"
expect_failure 4 TBV-ENV-SYNC /run_context/environment/synchronized_output_supported/value "$TMP/env-sync.json"
expect_failure 4 TBV-META-SYNC /meta/synchronized_output_supported "$TMP/meta-sync.json"
expect_failure 4 TBV-CONTROL-OS /run_context/environment/os/value "$TMP/os-noncanonical.json"
expect_failure 4 TBV-CONTROL-ARCH /run_context/environment/architecture/value "$TMP/arch-noncanonical.json"
expect_failure 4 TBV-CONTROL-POWER /run_context/environment/power_source/value "$TMP/power-noncanonical.json"
expect_failure 4 TBV-CONTROL-MACHINE /run_context/environment/machine_model/value "$TMP/machine-untrimmed.json"
expect_failure 4 TBV-CONTEXT-PARITY /run_context/environment/terminal/value "$TMP/terminal-parity.json"
expect_failure 4 TBV-CONTEXT-PARITY /run_context/environment/display_refresh_hz/value "$TMP/meta-refresh-far.json"
for duplicate in classification terminal median; do
  expect_failure 3 TBV-IO-DUPLICATE '' "$TMP/duplicate-$duplicate.json"
done
expect_failure 4 TBV-NUMBER-RANGE /results/0/median "$TMP/huge-median.json"
if [[ "$PORTABLE" != 1 ]]; then
  ! grep -Fq 'panicked' "$TMP/err" || fail "huge integer emitted a panic"
  [[ $(wc -c <"$TMP/err") -lt 2048 ]] || fail "huge integer diagnostic was not bounded"
fi
pass "table-driven mutations enforce structure, numbers, profiles, dimensions, and workload invariants"

"$VALIDATOR" validate "$TMP/proxy-note-absent.json" >/dev/null
"$VALIDATOR" validate "$TMP/workload-unknown.json" >/dev/null
"$VALIDATOR" validate "$TMP/meta-refresh-near.json" >/dev/null
pass "absent proxy notes, unknown workloads, and sub-0.1 Hz metadata drift remain conformant"

"$VALIDATOR" compare "$VALID" "$VALID" >"$TMP/comparable.out"
expect_status COMPARABLE "$TMP/comparable.out"
"$VALIDATOR" compare "$VALID" "$TMP/other-terminal.json" >"$TMP/other-terminal.out"
expect_status COMPARABLE "$TMP/other-terminal.out"
"$VALIDATOR" compare "$TMP/smoke.json" "$TMP/refresh-near.json" >"$TMP/refresh-near.out"
expect_status COMPARABLE "$TMP/refresh-near.out"
set +e
"$VALIDATOR" compare "$VALID" "$TMP/not-comparable.json" >"$TMP/not.out"
not_code=$?
"$VALIDATOR" compare "$VALID" "$TMP/undetermined.json" >"$TMP/unknown.out"
unknown_code=$?
"$VALIDATOR" compare "$VALID" "$TMP/workload-unknown.json" >"$TMP/workload-unknown.out"
workload_unknown_code=$?
"$VALIDATOR" compare "$TMP/smoke.json" "$TMP/refresh-far.json" >"$TMP/refresh-far.out"
refresh_far_code=$?
set -e
[[ $not_code -ne 0 ]] || fail "known mismatch was accepted"
[[ $unknown_code -ne 0 ]] || fail "unknown fact was accepted as comparable"
[[ $workload_unknown_code -ne 0 ]] || fail "unknown workload was accepted as comparable"
[[ $refresh_far_code -ne 0 ]] || fail "display refresh mismatch was accepted"
expect_status NOT_COMPARABLE "$TMP/not.out"
expect_status UNDETERMINED "$TMP/unknown.out"
expect_status UNDETERMINED "$TMP/workload-unknown.out"
expect_status NOT_COMPARABLE "$TMP/refresh-far.out"
if [[ "$PORTABLE" != 1 ]]; then
  [[ $not_code -eq 7 ]] || fail "known mismatch exit $not_code, expected 7"
  [[ $unknown_code -eq 8 ]] || fail "unknown fact exit $unknown_code, expected 8"
  [[ $workload_unknown_code -eq 8 ]] || fail "unknown workload exit $workload_unknown_code, expected 8"
  [[ $refresh_far_code -eq 7 ]] || fail "display refresh mismatch exit $refresh_far_code, expected 7"
fi
pass "pairwise gate separates COMPARABLE, NOT_COMPARABLE, and UNDETERMINED"

if [[ "$PORTABLE" != 1 ]]; then
  "$TBENCH" compare "$VALID" "$VALID" >/dev/null
  "$TBENCH" compare "$TMP/legacy.json" "$TMP/legacy.json" >/dev/null
fi
expect_failure 4 TBV-STRUCT-REQUIRED '/$schema' "$TMP/legacy.json"
if [[ "$PORTABLE" != 1 ]]; then
  pass "additive v1 and legacy reports remain readable by compare; legacy is not v1 conformant"
else
  pass "legacy report is not v1 conformant"
fi

if [[ "$PORTABLE" != 1 ]]; then
grep -Fq 'fixtures/conformance/v1/valid-minimal.json' "$ROOT/README.md" || fail "README does not link the valid v1 fixture"
grep -Fq 'bin/tbench-validate validate results/a.json &&' "$ROOT/README.md" || fail "README quickstart is not fail-closed"
grep -Fq 'bin/tbench-validate validate results/report-a.json &&' "$ROOT/docs/reporting-spec-v1.md" || fail "spec quickstart is not fail-closed"
mkdir -p "$TMP/workflow/bin"
cat >"$TMP/workflow/bin/tbench-validate" <<'SH'
#!/bin/zsh
if [[ "$1" == validate ]]; then
  exit 0
fi
if [[ "$1" == compare ]]; then
  print -- '{"status":"BLOCKED_BY_STUB"}'
  exit "${TBENCH_STUB_COMPARE_EXIT:?}"
fi
exit 2
SH
cat >"$TMP/workflow/bin/tbench" <<'SH'
#!/bin/zsh
print -- called >"${TBENCH_STUB_MARKER:?}"
SH
chmod +x "$TMP/workflow/bin/tbench-validate" "$TMP/workflow/bin/tbench"
for gate_exit in 7 8; do
  marker="$TMP/workflow/legacy-$gate_exit.called"
  set +e
  TBENCH_STUB_COMPARE_EXIT="$gate_exit" TBENCH_STUB_MARKER="$marker" zsh -c '
    cd "$1"
    set -e
    bin/tbench-validate validate a.json &&
    bin/tbench-validate validate b.json &&
    bin/tbench-validate compare a.json b.json &&
    bin/tbench compare a.json b.json
  ' _ "$TMP/workflow" >"$TMP/workflow-$gate_exit.out" 2>"$TMP/workflow-$gate_exit.err"
  workflow_code=$?
  set -e
  [[ $workflow_code -eq $gate_exit ]] || fail "fail-closed workflow returned $workflow_code, expected $gate_exit"
  [[ ! -e "$marker" ]] || fail "legacy compare ran after eligibility exit $gate_exit"
done
pass "documented quickstart stops before legacy compare on exits 7 and 8"
fi

if [[ "$PORTABLE" != 1 ]]; then
sh -n "$ROOT/bin/tbench"
sh -n "$ROOT/bin/tbench-validate"
! grep -Fq 'same as above' "$ROOT/docs/reporting-spec-v1.md" || fail "profile registry contains ambiguous shorthand"
git -C "$ROOT" diff --check
"$VALIDATOR" --help >"$TMP/help"
grep -Fq 'docs/reporting-spec-v1.md' "$TMP/help" || fail "validator help lacks specification quickstart path"
set +e
"$VALIDATOR" >/dev/null 2>&1
usage_code=$?
set -e
[[ $usage_code -eq 2 ]] || fail "no-argument usage exit is $usage_code, expected 2"
pass "launcher syntax and validator help/usage behavior pass"
fi

print -- "report-v1: PASS ($pass_count checks)"

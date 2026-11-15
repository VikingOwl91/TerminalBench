//! End-to-end run inside a pseudo-terminal the test owns. CI runners have
//! no controlling tty, so without this every tty-bound scenario would be
//! skipped there and only ever exercised on a developer's machine. The
//! test process holds the pty master, which also makes it the "terminal"
//! that resource.* discovers and samples.
#![cfg(target_os = "linux")]

use std::fs::File;
use std::io::Read;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// One `tbench run` at a time: they share results/.tbench.lock.
static SERIAL: Mutex<()> = Mutex::new(());

const COLS: u16 = 80;
const ROWS: u16 = 24;

fn open_pty() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave) = (-1, -1);
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    size.ws_col = COLS;
    size.ws_row = ROWS;
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &size,
        )
    };
    assert_eq!(rc, 0, "openpty failed");
    unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) }
}

/// Spawn tbench with the pty slave as its controlling terminal and keep
/// reading the master so writes to the terminal never block.
fn spawn_in_pty(args: &[&str], envs: &[(&str, &str)]) -> Child {
    let (master, slave) = open_pty();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tbench"));
    cmd.args(args)
        .envs(envs.iter().copied())
        .env_remove("TMUX")
        .env_remove("STY")
        .env_remove("TERM_PROGRAM")
        .env_remove("TBENCH_TERMINAL_ID")
        .stdin(Stdio::from(slave))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn().expect("spawn tbench");
    std::thread::spawn(move || {
        let mut master = File::from(master);
        let mut buf = vec![0u8; 1 << 16];
        while matches!(master.read(&mut buf), Ok(n) if n > 0) {}
    });
    child
}

fn temp_report(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tbench-pty-{}-{tag}.json", std::process::id()))
}

#[test]
fn every_scenario_produces_results_inside_a_pty() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let out = temp_report("full");
    let _ = std::fs::remove_file(&out);
    let status = spawn_in_pty(
        &[
            "run",
            "--shells",
            "bash",
            "--runs",
            "2",
            "--warmup",
            "0",
            "--settle",
            "0",
            "--strict",
            "--output",
            out.to_str().unwrap(),
        ],
        &[("FIRE_SECS", "0.2")],
    )
    .wait()
    .expect("wait for tbench");
    assert!(
        status.success(),
        "run --strict failed ({status}): a scenario produced no results"
    );

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("report written")).unwrap();
    let _ = std::fs::remove_file(&out);
    let rows = report["results"].as_array().unwrap();
    let has = |metric: &str, variant: Option<&str>| {
        rows.iter().any(|r| {
            r["metric"] == metric
                && match variant {
                    Some(v) => r["variant"] == v,
                    None => true,
                }
        })
    };

    for (metric, variant) in [
        ("shell.startup", None),
        ("cmd.overhead", None),
        ("render.throughput", Some("plain")),
        ("render.throughput", Some("ansi")),
        ("render.throughput", Some("cjk")),
        ("scroll.proxy", None),
        ("resource.mem", Some("idle")),
        ("resource.mem", Some("load")),
        ("resource.cpu", Some("idle")),
        ("resource.cpu", Some("load")),
        ("fire.fps", Some("80x24")),
    ] {
        assert!(has(metric, variant), "missing {metric} {variant:?}");
    }

    // resource.* found this test process as the pty owner and ran first.
    assert_eq!(rows[0]["metric"], "resource.mem");
    assert_eq!(rows[0]["backend"], "procfs");
    let resource = &report["extensions"]["org.terminalbench.resource"];
    assert_eq!(resource["pid"], std::process::id());
    assert_eq!(resource["discovery"], "pty-master-owner");

    let run = &report["extensions"]["org.terminalbench.run"];
    assert_eq!(run["tty"]["cols"], COLS);
    assert_eq!(run["tty"]["rows"], ROWS);
    assert_eq!(
        run["scenarios_without_results"].as_array().unwrap().len(),
        0
    );

    let env = &report["run_context"]["environment"];
    assert_eq!(env["terminal"]["status"], "known");
    assert_eq!(report["meta"]["meta_notes"], serde_json::Value::Null);
}

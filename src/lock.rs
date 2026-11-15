//! results/.tbench.lock — concurrent-run detection (proposal §A item 2).
//! One live `tbench run` at a time: a second run waits (up to 120s) rather
//! than cross-polluting the first's measurements; a stale lock (dead pid)
//! is reclaimed automatically.

use std::path::{Path, PathBuf};

pub struct Lock {
    path: PathBuf,
    owned: bool,
}

fn read_lock_pid(path: &Path) -> Option<i32> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().next()?.trim().parse().ok()
}

fn pid_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn acquire(path: &Path, force: bool) -> Result<Lock, String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    if let Some(existing_pid) = read_lock_pid(path) {
        if pid_alive(existing_pid) {
            if force {
                eprintln!("tbench: another tbench run (pid {existing_pid}) appears active; --force given, continuing anyway (results may be polluted by cross-run interference)");
            } else {
                eprintln!("tbench: another tbench run (pid {existing_pid}) appears active; waiting for it to finish (up to 120s; use --force to skip this check)...");
                let mut waited = 0;
                while waited < 120 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    waited += 2;
                    match read_lock_pid(path) {
                        None => break,
                        Some(pid) if !pid_alive(pid) => break,
                        Some(_) => {}
                    }
                }
                if let Some(pid) = read_lock_pid(path) {
                    if pid_alive(pid) {
                        return Err(format!(
                            "tbench: timed out waiting for pid {pid} to finish; aborting (use --force to skip this check)"
                        ));
                    }
                }
            }
        } else {
            eprintln!(
                "tbench: found a stale lock (pid {existing_pid} is no longer running); reclaiming"
            );
        }
    }

    let content = format!(
        "{}\n{}\n",
        std::process::id(),
        crate::report::utc_timestamp()
    );
    std::fs::write(path, content).map_err(|e| format!("tbench: failed to write lock: {e}"))?;
    crate::cleanup::set_lock(path);
    Ok(Lock {
        path: path.to_path_buf(),
        owned: true,
    })
}

impl Drop for Lock {
    fn drop(&mut self) {
        crate::cleanup::clear_lock();
        // Only remove a lock this process itself owns.
        if self.owned {
            if let Some(pid) = read_lock_pid(&self.path) {
                if pid == std::process::id() as i32 {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
        }
    }
}

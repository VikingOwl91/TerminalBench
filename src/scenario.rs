//! Scenario loading. scenarios/*.yaml keep the fixed flat `key: value`
//! structure (kind/metric/unit/command/description/notes + the
//! resource-footprint key pairs); the naive line parser here matches
//! lib/runner.sh's `tbench_scenario_field` so the same files drive both
//! implementations during the migration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String, // file stem, e.g. "shell-startup"
    pub path: PathBuf,
    fields: HashMap<String, String>,
}

impl Scenario {
    pub fn load(path: &Path) -> std::io::Result<Scenario> {
        let text = std::fs::read_to_string(path)?;
        let mut fields = HashMap::new();
        for line in text.lines() {
            // First `key: value` wins, matching `head -n1` in the zsh parser.
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                if key.is_empty() || key.contains(char::is_whitespace) {
                    continue;
                }
                let mut value = value.trim().to_string();
                if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
                    value = value[1..value.len() - 1].to_string();
                }
                fields.entry(key.to_string()).or_insert(value);
            }
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Scenario {
            name,
            path: path.to_path_buf(),
            fields,
        })
    }

    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
    }

    pub fn kind(&self) -> &str {
        self.field("kind").unwrap_or("shell")
    }
}

/// All scenarios/*.yaml under the given root, sorted by filename (matching
/// the zsh glob's lexicographic order, which fixes the run order).
pub fn load_all(root: &Path) -> std::io::Result<Vec<Scenario>> {
    let dir = root.join("scenarios");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    paths.sort();
    paths.iter().map(|p| Scenario::load(p)).collect()
}

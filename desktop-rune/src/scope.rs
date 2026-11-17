use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// AllExcept follows newly configured repositories; Only is a fixed, explicit subset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "repositories", rename_all = "snake_case")]
pub enum RepoScope {
    AllExcept(BTreeSet<String>),
    Only(BTreeSet<String>),
}
impl Default for RepoScope {
    fn default() -> Self {
        Self::AllExcept(BTreeSet::new())
    }
}
impl RepoScope {
    pub fn only(repo: impl Into<String>) -> Self {
        Self::Only([repo.into()].into())
    }
    pub fn includes(&self, repo: &str) -> bool {
        match self {
            Self::AllExcept(excluded) => !excluded.contains(repo),
            Self::Only(included) => included.contains(repo),
        }
    }
    pub fn toggle(&mut self, repo: &str) {
        match self {
            Self::AllExcept(set) | Self::Only(set) => {
                if !set.remove(repo) {
                    set.insert(repo.into());
                }
            }
        }
    }
    pub fn selected(&self, available: &[String]) -> Vec<String> {
        available
            .iter()
            .filter(|repo| self.includes(repo))
            .cloned()
            .collect()
    }
    pub fn summary(&self, available: &[String]) -> String {
        let selected = self.selected(available);
        if selected.len() == 1 {
            return crate::data::repo_name(&selected[0]).into();
        }
        match self {
            Self::AllExcept(_) => {
                let excluded = available.len() - selected.len();
                if excluded == 0 {
                    "All repositories".into()
                } else if selected.is_empty() {
                    "No repositories selected".into()
                } else {
                    format!("All except {excluded}")
                }
            }
            Self::Only(_) => {
                if selected.is_empty() {
                    "No repositories selected".into()
                } else {
                    format!("{} repositories selected", selected.len())
                }
            }
        }
    }
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let save = || -> Result<(), Box<dyn std::error::Error>> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = path.with_extension("json.new");
            std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
            std::fs::rename(temporary, path)?;
            Ok(())
        };
        save().map_err(|e| format!("Could not save repository selection: {e}"))
    }
}

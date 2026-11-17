use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

fn yes() -> bool {
    true
}
fn interval() -> u64 {
    300
}
pub fn normalize_terms(terms: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    terms.iter().map(|term| term.trim().to_lowercase()).filter(|term| !term.is_empty() && seen.insert(term.clone())).collect()
}
pub fn includes(terms: &[String], title: &str) -> bool {
    if terms.is_empty() { return true; }
    let title=title.to_lowercase();
    terms.iter().any(|term|title.contains(term))
}
fn roster() -> String {
    "maintainers.json".into()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Repository {
    pub name: String,
    #[serde(default, rename = "includeTerms")]
    pub include_terms: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Maintainer {
    #[serde(default, rename = "githubId")]
    pub github_id: String,
    pub login: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Person {
    #[serde(rename = "githubId")]
    pub github_id: String,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub customers: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "interval", rename = "pollIntervalSeconds")]
    pub interval: u64,
    #[serde(default = "roster", rename = "maintainersFile")]
    pub maintainers_file: String,
    #[serde(default)]
    pub repositories: Vec<Repository>,
    #[serde(skip)]
    pub maintainers: Vec<Maintainer>,
    #[serde(skip)]
    pub people: Vec<Person>,
}
impl Config {
    pub fn load(directory: &Path, override_path: Option<&Path>) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (name, embedded) in [
            (
                "tracker.config.json",
                include_str!("../../tracker.config.json"),
            ),
            ("maintainers.json", include_str!("../../maintainers.json")),
        ] {
            let dest = directory.join(name);
            if !dest.exists() {
                std::fs::write(
                    dest,
                    std::fs::read(project.join(name))
                        .unwrap_or_else(|_| embedded.as_bytes().to_vec()),
                )?;
            }
        }
        let people_dest = directory.join("people.local.json");
        if !people_dest.exists() && project.join("people.local.json").exists() {
            std::fs::copy(project.join("people.local.json"), &people_dest)?;
        }
        let path = override_path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| directory.join("tracker.config.json"));
        let mut config: Self = serde_json::from_slice(
            &std::fs::read(&path).context("Cannot read tracker configuration")?,
        )?;
        let base = path.parent().unwrap_or(directory);
        let roster_path=base.join(&config.maintainers_file);
        config.maintainers=crate::maintainer_config::read_valid(&roster_path)
            .or_else(|_|crate::maintainer_config::read_valid(&roster_path.with_extension("last-good.json")))
            .unwrap_or_default();
        let people = std::env::var_os("TRACKER_PEOPLE_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| base.join("people.local.json"));
        if people.exists() {
            let data: serde_json::Value = serde_json::from_slice(&std::fs::read(people)?)?;
            config.people = serde_json::from_value(data["people"].clone())?;
        }
        config.validate()?;
        let repository_path = directory.join("repositories.json");
        if repository_path.exists() {
            let read = |path: &Path| -> Result<Config> {
                let file: crate::repository_config::RepositoryFile =
                    serde_json::from_slice(&std::fs::read(path)?)
                        .context("Invalid repositories.json")?;
                crate::repository_config::validated(file.repositories, &config)
            };
            config = read(&repository_path)
                .or_else(|_| read(&directory.join("repositories.last-good.json")))
                .unwrap_or(config);
        } else {
            crate::repository_config::write_file(&repository_path, &config.repositories)?;
        }
        Ok(config)
    }
    pub fn validate(&mut self) -> Result<()> {
        if self.interval < 300 {
            bail!("Polling interval must be at least 300 seconds");
        }
        let valid = regex::Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")?;
        let mut repos = HashSet::new();
        for repo in &mut self.repositories {
            repo.name = repo.name.to_lowercase();
            repo.include_terms = normalize_terms(&repo.include_terms);
            if !valid.is_match(&repo.name)
                || !repos.insert(repo.name.clone())
            {
                bail!("Invalid or duplicate repository: {}", repo.name);
            }
        }
        for ids in [
            self.maintainers
                .iter()
                .map(|p| p.github_id.as_str())
                .collect::<Vec<_>>(),
            self.people.iter().map(|p| p.github_id.as_str()).collect(),
        ] {
            let mut seen = HashSet::new();
            for id in ids {
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || !seen.insert(id) {
                    bail!("Invalid or duplicate identity ID");
                }
            }
        }
        if self.maintainers.iter().any(|m| m.login.trim().is_empty())
            || self
                .people
                .iter()
                .any(|p| p.customers.iter().any(|c| c.trim().is_empty()))
        {
            bail!("Maintainer logins and customer names cannot be empty");
        }
        Ok(())
    }
}
pub fn token() -> Option<String> {
    if let Ok(value) = std::env::var("GITHUB_TOKEN") {
        if !value.trim().is_empty() {
            return Some(value.trim().into());
        }
    }
    None
}
pub fn data_dir() -> PathBuf {
    std::env::var_os("TRACKER_DESKTOP_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let support=PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Library/Application Support");
            let current=support.join("Threadkeeper");
            let legacy=support.join("PowerSync Tracker");
            if !current.exists() && legacy.exists() {legacy} else {current}
        })
}

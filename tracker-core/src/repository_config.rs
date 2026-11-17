use crate::{
    config::{Config, Repository},
    github::{SharedGitHub, Source},
    store::Store,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryFile {
    pub repositories: Vec<Repository>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepositorySettings {
    pub repositories: Vec<Repository>,
    pub revision: String,
    pub path: String,
    pub error: Option<String>,
}
fn revision(raw: &[u8]) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
pub fn normalize_name(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let name = if trimmed.starts_with("https://") {
        let url = reqwest::Url::parse(trimmed)?;
        if url.host_str() != Some("github.com") {
            bail!("Use a github.com repository URL or owner/repo");
        }
        url.path()
            .trim_matches('/')
            .trim_end_matches(".git")
            .to_owned()
    } else {
        trimmed.trim_end_matches(".git").to_owned()
    };
    if !regex::Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")?.is_match(&name) {
        bail!("Enter a repository as owner/repo or https://github.com/owner/repo");
    }
    Ok(name.to_lowercase())
}
pub fn validated(repositories: Vec<Repository>, base: &Config) -> Result<Config> {
    let mut next = base.clone();
    next.repositories = repositories;
    for repo in &mut next.repositories {
        repo.name = normalize_name(&repo.name)?;
    }
    next.validate()?;
    Ok(next)
}
pub fn write_file(path: &Path, repositories: &[Repository]) -> Result<()> {
    let mut raw = serde_json::to_vec_pretty(&RepositoryFile {
        repositories: repositories.to_vec(),
    })?;
    raw.push(b'\n');
    let temporary = path.with_extension("json.new");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    use std::io::Write;
    file.write_all(&raw)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)?;
    Ok(())
}
struct Active {
    config: Config,
    last_seen: Vec<u8>,
    error: Option<String>,
}
pub struct RepositoryManager {
    pub path: PathBuf,
    active: Mutex<Active>,
}
impl RepositoryManager {
    pub fn new(path: PathBuf, config: Config) -> Result<Self> {
        let raw = std::fs::read(&path)?;
        let error = serde_json::from_slice::<RepositoryFile>(&raw)
            .map_err(anyhow::Error::from)
            .and_then(|file| validated(file.repositories, &config))
            .err()
            .map(|e| format!("Repository configuration: {e:#}"));
        if error.is_none() {
            let _ = write_file(
                &path.with_file_name("repositories.last-good.json"),
                &config.repositories,
            );
        }
        Ok(Self {
            path,
            active: Mutex::new(Active {
                config,
                last_seen: raw,
                error,
            }),
        })
    }
    pub fn current_error(&self) -> Option<String> {
        self.active
            .try_lock()
            .ok()
            .and_then(|active| active.error.clone())
    }
    pub async fn settings(&self) -> RepositorySettings {
        let active = self.active.lock().await;
        RepositorySettings {
            repositories: active.config.repositories.clone(),
            revision: revision(&active.last_seen),
            path: self.path.to_string_lossy().into(),
            error: active.error.clone(),
        }
    }
    pub async fn reload(&self, store: &Store) -> bool {
        let Ok(mut active) = self.active.try_lock() else {
            return false;
        };
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(error) => {
                let error = format!("Repository configuration: {error}");
                let changed = active.error.as_ref() != Some(&error);
                active.error = Some(error);
                active.last_seen.clear();
                return changed;
            }
        };
        if raw == active.last_seen {
            return false;
        }
        active.last_seen = raw.clone();
        let result = async {
            let document: RepositoryFile =
                serde_json::from_slice(&raw).context("Invalid repositories.json")?;
            let next = validated(document.repositories, &active.config)?;
            apply_tracking(store, &active.config, &next).await?;
            Ok::<_, anyhow::Error>(next)
        }
        .await;
        match result {
            Ok(next) => {
                let _ = write_file(
                    &self.path.with_file_name("repositories.last-good.json"),
                    &next.repositories,
                );
                active.config = next;
                active.error = None;
            }
            Err(error) => active.error = Some(format!("Repository configuration: {error:#}")),
        }
        true
    }
    pub async fn save(
        &self,
        store: &Store,
        repositories: Vec<Repository>,
        expected: &str,
        github: Option<SharedGitHub>,
    ) -> Result<RepositorySettings> {
        let mut active = self.active.lock().await;
        if revision(&std::fs::read(&self.path).unwrap_or_default()) != expected {
            bail!(
                "The repository file changed while preferences were open. Reload the file before saving."
            );
        }
        let next = validated(repositories, &active.config)?;
        for repo in &next.repositories {
            // Existing enabled repositories can be edited or removed even while offline.
            if repo.enabled
                && !active
                    .config
                    .repositories
                    .iter()
                    .any(|old| old.name == repo.name && old.enabled)
            {
                let mut github = github.clone().context(
                    "Provide a GitHub token and reopen the app to validate new repositories",
                )?;
                github
                    .repository(&repo.name)
                    .await
                    .with_context(|| format!("Cannot access {}", repo.name))?;
            }
        }
        if revision(&std::fs::read(&self.path).unwrap_or_default()) != expected {
            bail!("The repository file changed during validation. Reload the file before saving.");
        }
        write_file(&self.path, &next.repositories)
            .context("Could not save repository configuration")?;
        apply_tracking(store, &active.config, &next).await?;
        active.last_seen = std::fs::read(&self.path)?;
        let _ = write_file(
            &self.path.with_file_name("repositories.last-good.json"),
            &next.repositories,
        );
        active.config = next;
        active.error = None;
        Ok(RepositorySettings {
            repositories: active.config.repositories.clone(),
            revision: revision(&active.last_seen),
            path: self.path.to_string_lossy().into(),
            error: None,
        })
    }
}

async fn apply_tracking(store: &Store, old: &Config, next: &Config) -> Result<()> {
    store.configure_repositories(next).await?;
    for repo in &next.repositories {
        if repo.enabled
            && !old
                .repositories
                .iter()
                .any(|r| r.name == repo.name && r.enabled)
        {
            let row = store.repository(&repo.name).await?;
            let key = crate::store::string(&row, "id");
            if !store.poll_state(key).await?.is_null() {
                store
                    .update_poll(
                        key,
                        serde_json::json!({"error":null,"next_retry":null,"failures":0}),
                    )
                    .await?;
            }
        }
    }
    Ok(())
}

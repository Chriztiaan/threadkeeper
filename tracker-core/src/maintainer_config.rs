use crate::{config::Maintainer, github::SharedGitHub, store::Store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaintainerFile {
    pub maintainers: Vec<Maintainer>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MaintainerSettings {
    pub maintainers: Vec<Maintainer>,
    pub revision: String,
    pub path: String,
    pub error: Option<String>,
}
fn revision(raw: &[u8]) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
pub fn normalize_login(value: &str) -> Result<String> {
    let login = value.trim().trim_start_matches('@');
    let login = if login.starts_with("https://github.com/") {
        login
            .trim_start_matches("https://github.com/")
            .trim_end_matches('/')
    } else {
        login
    };
    if !regex::Regex::new(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?$")?.is_match(login)
        || login.contains("--")
    {
        bail!("Enter a GitHub username or profile URL");
    }
    Ok(login.into())
}
pub fn validated(mut maintainers: Vec<Maintainer>, require_ids: bool) -> Result<Vec<Maintainer>> {
    let mut logins = HashSet::new();
    let mut ids = HashSet::new();
    for person in &mut maintainers {
        person.login = normalize_login(&person.login)?;
        if !logins.insert(person.login.to_lowercase()) {
            bail!("Duplicate maintainer: {}", person.login);
        }
        if person.github_id.is_empty() {
            if require_ids {
                bail!(
                    "Maintainer {} needs a stable githubId; import and save through Settings to resolve usernames",
                    person.login
                );
            }
        } else if !person.github_id.bytes().all(|b| b.is_ascii_digit())
            || person.github_id.starts_with('0')
            || !ids.insert(person.github_id.clone())
        {
            bail!("Invalid or duplicate maintainer GitHub ID");
        }
    }
    Ok(maintainers)
}
pub fn read_valid(path: &Path) -> Result<Vec<Maintainer>> {
    validated(
        serde_json::from_slice::<MaintainerFile>(&std::fs::read(path)?)?.maintainers,
        true,
    )
}
pub fn write_file(path: &Path, maintainers: &[Maintainer]) -> Result<()> {
    let mut raw = serde_json::to_vec_pretty(&MaintainerFile {
        maintainers: maintainers.to_vec(),
    })?;
    raw.push(b'\n');
    let temporary = path.with_extension("json.new");
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&raw)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, path)?;
    Ok(())
}
struct Active {
    maintainers: Vec<Maintainer>,
    last_seen: Vec<u8>,
    error: Option<String>,
}
pub struct MaintainerManager {
    pub path: PathBuf,
    active: Mutex<Active>,
}
impl MaintainerManager {
    pub fn new(path: PathBuf, maintainers: Vec<Maintainer>) -> Result<Self> {
        let raw = std::fs::read(&path).unwrap_or_default();
        let error = read_valid(&path)
            .err()
            .map(|e| format!("Maintainer configuration: {e:#}"));
        if error.is_none() {
            let _ = write_file(&path.with_extension("last-good.json"), &maintainers);
        }
        Ok(Self {
            path,
            active: Mutex::new(Active {
                maintainers,
                last_seen: raw,
                error,
            }),
        })
    }
    pub fn current_error(&self) -> Option<String> {
        self.active.try_lock().ok().and_then(|a| a.error.clone())
    }
    pub async fn settings(&self) -> MaintainerSettings {
        let a = self.active.lock().await;
        MaintainerSettings {
            maintainers: a.maintainers.clone(),
            revision: revision(&a.last_seen),
            path: self.path.to_string_lossy().into(),
            error: a.error.clone(),
        }
    }
    pub async fn reload(&self, store: &Store) -> bool {
        let Ok(mut a) = self.active.try_lock() else {
            return false;
        };
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(error) => {
                let error = format!("Maintainer configuration: {error}");
                let changed = a.error.as_ref() != Some(&error);
                a.error = Some(error);
                a.last_seen.clear();
                return changed;
            }
        };
        if raw == a.last_seen {
            return false;
        }
        a.last_seen = raw.clone();
        let next = serde_json::from_slice::<MaintainerFile>(&raw)
            .map_err(anyhow::Error::from)
            .and_then(|file| validated(file.maintainers, true));
        match next {
            Ok(next) => match store.configure_maintainers(&next).await {
                Ok(()) => {
                    let _ = write_file(&self.path.with_extension("last-good.json"), &next);
                    a.maintainers = next;
                    a.error = None;
                }
                Err(error) => a.error = Some(format!("Maintainer configuration: {error:#}")),
            },
            Err(error) => a.error = Some(format!("Maintainer configuration: {error:#}")),
        }
        true
    }
    pub async fn save(
        &self,
        store: &Store,
        maintainers: Vec<Maintainer>,
        expected: &str,
        github: Option<SharedGitHub>,
    ) -> Result<MaintainerSettings> {
        let mut a = self.active.lock().await;
        if revision(&std::fs::read(&self.path).unwrap_or_default()) != expected {
            bail!(
                "The maintainer file changed while Settings were open. Reload the file before saving."
            );
        }
        let mut next = validated(maintainers, false)?;
        for person in &mut next {
            if let Some(known) = a.maintainers.iter().find(|m| {
                m.login.eq_ignore_ascii_case(&person.login)
                    && (person.github_id.is_empty() || person.github_id == m.github_id)
            }) {
                person.github_id = known.github_id.clone();
                person.login = known.login.clone();
                continue;
            }
            let github = github.as_ref().context(
                "Provide a GitHub token and reopen the app to validate new maintainers",
            )?;
            let user = github
                .0
                .lock()
                .await
                .user(&person.login)
                .await
                .with_context(|| format!("Cannot resolve GitHub user {}", person.login))?;
            let id = user["id"]
                .as_u64()
                .context("Missing GitHub user ID")?
                .to_string();
            if !person.github_id.is_empty() && person.github_id != id {
                bail!("GitHub ID does not match username {}", person.login);
            }
            person.github_id = id;
            person.login = user["login"]
                .as_str()
                .context("Missing GitHub login")?
                .into();
        }
        let next = validated(next, true)?;
        if revision(&std::fs::read(&self.path).unwrap_or_default()) != expected {
            bail!("The maintainer file changed during validation. Reload the file before saving.");
        }
        write_file(&self.path, &next)?;
        store.configure_maintainers(&next).await?;
        a.last_seen = std::fs::read(&self.path)?;
        let _ = write_file(&self.path.with_extension("last-good.json"), &next);
        a.maintainers = next;
        a.error = None;
        Ok(MaintainerSettings {
            maintainers: a.maintainers.clone(),
            revision: revision(&a.last_seen),
            path: self.path.to_string_lossy().into(),
            error: None,
        })
    }
}

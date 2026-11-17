#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(from="LegacyPerson")]
pub struct Customer {
    #[serde(default, rename="githubId")]
    pub github_id: String,
    pub login: String,
    #[serde(default, rename="fullName")]
    pub full_name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub highlight: Highlight,
}
// Preserve legacy organization metadata while keeping company and notes independent.
#[derive(serde::Deserialize)]
struct LegacyPerson {
    #[serde(default,rename="githubId")] github_id:String,
    login:String,
    #[serde(default,rename="fullName")] full_name:String,
    #[serde(default)] notes:String,
    #[serde(default)] company:String,
    #[serde(default)] organization:String,
    #[serde(default)] highlight:Highlight,
}
impl From<LegacyPerson> for Customer {
    fn from(p:LegacyPerson)->Self {
        let mut companies=Vec::new();
        for value in [p.company,p.organization] {if !value.trim().is_empty() && !companies.contains(&value){companies.push(value);}}
        Self{github_id:p.github_id,login:p.login,full_name:p.full_name,notes:p.notes,company:companies.join(" / "),highlight:p.highlight}
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum Highlight { #[default] GoldStar, Neon, Outline, SubtleTint, BadgeOnly }
impl Highlight {
    pub const ALL:[Self;5]=[Self::GoldStar,Self::Neon,Self::Outline,Self::SubtleTint,Self::BadgeOnly];
    pub fn key(self)->&'static str {match self {Self::GoldStar=>"gold-star",Self::Neon=>"neon",Self::Outline=>"outline",Self::SubtleTint=>"subtle-tint",Self::BadgeOnly=>"badge-only"}}

    pub fn label(self)-> &'static str {match self {Self::GoldStar=>"★ Gold star",Self::Neon=>"Neon",Self::Outline=>"Outline",Self::SubtleTint=>"Subtle tint",Self::BadgeOnly=>"Badge only"}}
    pub fn next(self)->Self {match self {Self::GoldStar=>Self::Neon,Self::Neon=>Self::Outline,Self::Outline=>Self::SubtleTint,Self::SubtleTint=>Self::BadgeOnly,Self::BadgeOnly=>Self::GoldStar}}
}
use crate::{ github::SharedGitHub, store::Store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomerFile {
    #[serde(rename="vips",alias="customers")]
    pub customers: Vec<Customer>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomerSettings {
    #[serde(rename="vips",alias="customers")]
    pub customers: Vec<Customer>,
    pub revision: String,
    pub path: String,
    pub error: Option<String>,
}
fn revision(raw: &[u8]) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
pub use crate::maintainer_config::normalize_login;
pub fn validated(mut customers: Vec<Customer>, require_ids: bool) -> Result<Vec<Customer>> {
    let mut logins = HashSet::new();
    let mut ids = HashSet::new();
    for person in &mut customers {
        person.login = normalize_login(&person.login)?;
        if !logins.insert(person.login.to_lowercase()) {
            bail!("Duplicate VIP: {}", person.login);
        }
        if person.github_id.is_empty() {
            if require_ids {
                bail!(
                    "VIP {} needs a stable githubId; import and save through Settings to resolve usernames",
                    person.login
                );
            }
        } else if !person.github_id.bytes().all(|b| b.is_ascii_digit())
            || person.github_id.starts_with('0')
            || !ids.insert(person.github_id.clone())
        {
            bail!("Invalid or duplicate VIP GitHub ID");
        }
    }
    Ok(customers)
}
pub fn read_valid(path: &Path) -> Result<Vec<Customer>> {
    validated(
        serde_json::from_slice::<CustomerFile>(&std::fs::read(path)?)?.customers,
        true,
    )
}
pub fn write_file(path: &Path, customers: &[Customer]) -> Result<()> {
    let mut raw = serde_json::to_vec_pretty(&CustomerFile {
        customers: customers.to_vec(),
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
    customers: Vec<Customer>,
    last_seen: Vec<u8>,
    error: Option<String>,
}
pub struct CustomerManager {
    pub path: PathBuf,
    active: Mutex<Active>,
}
impl CustomerManager {
    pub fn new(path: PathBuf, customers: Vec<Customer>) -> Result<Self> {
        let raw = std::fs::read(&path).unwrap_or_default();
        let error = read_valid(&path)
            .err()
            .map(|e| format!("VIP configuration: {e:#}"));
        if error.is_none() {
            let _ = write_file(&path.with_extension("last-good.json"), &customers);
        }
        Ok(Self {
            path,
            active: Mutex::new(Active {
                customers,
                last_seen: raw,
                error,
            }),
        })
    }
    pub fn current_error(&self) -> Option<String> {
        self.active.try_lock().ok().and_then(|a| a.error.clone())
    }
    pub async fn settings(&self) -> CustomerSettings {
        let a = self.active.lock().await;
        CustomerSettings {
            customers: a.customers.clone(),
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
                let error = format!("VIP configuration: {error}");
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
        let next = serde_json::from_slice::<CustomerFile>(&raw)
            .map_err(anyhow::Error::from)
            .and_then(|file| validated(file.customers, true));
        match next {
            Ok(next) => match store.configure_customers(&next).await {
                Ok(()) => {
                    let _ = write_file(&self.path.with_extension("last-good.json"), &next);
                    a.customers = next;
                    a.error = None;
                }
                Err(error) => a.error = Some(format!("VIP configuration: {error:#}")),
            },
            Err(error) => a.error = Some(format!("VIP configuration: {error:#}")),
        }
        true
    }
    pub async fn save(
        &self,
        store: &Store,
        customers: Vec<Customer>,
        expected: &str,
        github: Option<SharedGitHub>,
    ) -> Result<CustomerSettings> {
        let mut a = self.active.lock().await;
        if revision(&std::fs::read(&self.path).unwrap_or_default()) != expected {
            bail!(
                "The VIP file changed while Settings were open. Reload the file before saving."
            );
        }
        let mut next = validated(customers, false)?;
        for person in &mut next {
            if let Some(known) = a.customers.iter().find(|m| {
                m.login.eq_ignore_ascii_case(&person.login)
                    && (person.github_id.is_empty() || person.github_id == m.github_id)
            }) {
                person.github_id = known.github_id.clone();
                person.login = known.login.clone();
                continue;
            }
            let github = github.as_ref().context(
                "Provide a GitHub token and reopen the app to validate new VIPs",
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
            bail!("The VIP file changed during validation. Reload the file before saving.");
        }
        write_file(&self.path, &next)?;
        store.configure_customers(&next).await?;
        a.last_seen = std::fs::read(&self.path)?;
        let _ = write_file(&self.path.with_extension("last-good.json"), &next);
        a.customers = next;
        a.error = None;
        Ok(CustomerSettings {
            customers: a.customers.clone(),
            revision: revision(&a.last_seen),
            path: self.path.to_string_lossy().into(),
            error: None,
        })
    }
}

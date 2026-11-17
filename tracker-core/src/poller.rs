use crate::{
    github::{GitHubError, Source},
    store::{Store, now, string},
};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RefreshStatus {
    pub status: String,
    pub message: String,
    pub repo: Option<String>,
    #[serde(default)]
    pub repositories: Option<Vec<String>>,
}
impl Default for RefreshStatus {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            message: String::new(),
            repo: None,
            repositories: None,
        }
    }
}
#[derive(Debug)]
struct RepositoryDisabled;
impl std::fmt::Display for RepositoryDisabled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Repository no longer tracked")
    }
}
impl std::error::Error for RepositoryDisabled {}
pub type Status = Arc<std::sync::Mutex<RefreshStatus>>;
pub struct Poller<G: Source> {
    pub store: Arc<Store>,
    source: G,
    interval: u64,
    metadata: HashMap<String, i64>,
    paused: HashSet<String>,
    cursor: usize,
    pub stopped: Arc<AtomicBool>,
    pub status: Status,
}
fn elapsed(at: &Value, seconds: u64) -> bool {
    at.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_none_or(|t| Utc::now() - t.to_utc() >= Duration::seconds(seconds as i64))
}
fn retry_future(at: &Value) -> bool {
    at.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|t| t.to_utc() > Utc::now())
}
impl<G: Source> Poller<G> {
    pub fn new(
        store: Arc<Store>,
        source: G,
        interval: u64,
        stopped: Arc<AtomicBool>,
        status: Status,
    ) -> Self {
        Self {
            store,
            source,
            interval,
            metadata: HashMap::new(),
            paused: HashSet::new(),
            cursor: 0,
            stopped,
            status,
        }
    }
    async fn ensure_active(&self, name: &str) -> Result<()> {
        if self.store.repository(name).await?["enabled"] != 1 {
            return Err(RepositoryDisabled.into());
        }
        Ok(())
    }
    fn cancelled(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }
    fn progress(&self, message: String) {
        self.status.lock().unwrap().message = message;
    }
    async fn enrich(&mut self, repo: &str, items: &[Value]) -> Result<HashMap<u64, Value>> {
        let mut result = HashMap::new();
        for item in items {
            self.ensure_active(repo).await?;
            if self.cancelled() {
                anyhow::bail!("Import stopped");
            }
            if !item["pull_request"].is_null() && self.store.should_enrich(repo,item).await? {
                let n = item["number"].as_u64().unwrap_or_default();
                result.insert(n, self.source.pull(repo, n).await?);
            }
        }
        Ok(result)
    }
    pub async fn step(&mut self, name: &str, force: bool) -> Result<()> {
        if self.cancelled() {
            return Ok(());
        }
        let repo = self.store.repository(name).await?;
        let state = self.store.poll_state(string(&repo, "id")).await?;
        if self.paused.contains(name) {
            if state["error"].is_null() {
                self.paused.remove(name);
                self.metadata.remove(name);
            } else {
                return Ok(());
            }
        }
        if retry_future(&state["next_retry"]) {
            return Ok(());
        }
        let started = now();
        let result = self.import(name, force, &started).await;
        if let Err(error) = result {
            if error.is::<RepositoryDisabled>() {
                self.store.execute("UPDATE poll_state SET active=0 WHERE id IN (SELECT id FROM repositories WHERE configured_name=?)",&[json!(name)]).await?;
                return Ok(());
            }
            let repo = self.store.repository(name).await?;
            let key = string(&repo, "id");
            let state = self.store.poll_state(key).await?;
            let failures = state["failures"].as_u64().unwrap_or(0) + 1;
            let github = error.downcast_ref::<GitHubError>();
            let permanent = github.is_some_and(|e| matches!(e.status, 401 | 403 | 404));
            let retry = github
                .map(|e| e.retry_at)
                .filter(|t| *t > 0)
                .unwrap_or_else(|| {
                    Utc::now().timestamp_millis()
                        + (60000i64 * (1i64 << failures.min(6).saturating_sub(1))).min(1800000)
                });
            let message = github
                .map(|e| e.message.as_str())
                .unwrap_or("Import interrupted; checkpoint retained for retry");
            self.store.update_poll(key,json!({"active":0,"error":message,"failures":failures,"last_attempt":started,"next_retry":if permanent{Value::Null}else{json!(DateTime::from_timestamp_millis(retry).unwrap().to_rfc3339())}})).await?;
            if permanent {
                self.paused.insert(name.into());
            }
        }
        Ok(())
    }
    async fn import(&mut self, name: &str, force: bool, started: &str) -> Result<()> {
        self.ensure_active(name).await?;
        let mut repo = self.store.repository(name).await?;
        let mut state = self.store.poll_state(string(&repo, "id")).await?;
        if state.is_null()
            || self
                .metadata
                .get(name)
                .is_none_or(|t| Utc::now().timestamp() - t > 86400)
        {
            let meta = self.source.repository(name).await?;
            repo = self.store.save_repository(name, &meta).await?;
            self.metadata.insert(name.into(), Utc::now().timestamp());
            state = self.store.poll_state(string(&repo, "id")).await?;
        }
        self.ensure_active(name).await?;
        let key = string(&repo, "id");
        let canonical = string(&repo, "name");
        let due = elapsed(&state["last_success"], self.interval);
        if !force
            && state["stage"] == "ready"
            && !due
            && state["reconcile_page"].is_null()
            && !elapsed(&state["last_reconcile"], 86400)
        {
            return Ok(());
        }
        self.store
            .update_poll(key, json!({"last_attempt":started,"active":1}))
            .await?;
        if state["stage"] == "opening" {
            let page = self
                .source
                .issues(
                    canonical,
                    "open",
                    state["open_page"].as_u64().unwrap_or(1),
                    None,
                    "created",
                )
                .await?;
            let pulls = self.enrich(canonical, &page.items).await?;
            self.store.save_items(&repo, &page.items, &pulls).await?;
            self.store.update_poll(key,if let Some(next)=page.next{json!({"open_page":next})}else{json!({"stage":"backfill","checkpoint":state["initial_at"],"last_success":now()})}).await?;
        } else if force || due {
            let base = state["checkpoint"]
                .as_str()
                .or(state["initial_at"].as_str())
                .unwrap_or(started);
            let since =
                (DateTime::parse_from_rfc3339(base)?.to_utc() - Duration::minutes(5)).to_rfc3339();
            let mut number = Some(1);
            while let Some(n) = number {
                self.ensure_active(name).await?;
                if self.cancelled() {
                    anyhow::bail!("Import stopped");
                }
                let page = self
                    .source
                    .issues(canonical, "all", n, Some(&since), "updated")
                    .await?;
                let pulls = self.enrich(canonical, &page.items).await?;
                self.store.save_items(&repo, &page.items, &pulls).await?;
                number = page.next;
            }
            self.store
                .update_poll(key, json!({"checkpoint":started,"last_success":now()}))
                .await?;
        } else if state["stage"] == "backfill" {
            let page = self
                .source
                .issues(
                    canonical,
                    "all",
                    state["backfill_page"].as_u64().unwrap_or(1),
                    None,
                    "created",
                )
                .await?;
            let pulls = self.enrich(canonical, &page.items).await?;
            self.store.save_items(&repo, &page.items, &pulls).await?;
            self.store
                .update_poll(
                    key,
                    if let Some(next) = page.next {
                        json!({"backfill_page":next})
                    } else {
                        json!({"stage":"ready","last_reconcile":started})
                    },
                )
                .await?;
        } else if !state["reconcile_page"].is_null() || elapsed(&state["last_reconcile"], 86400) {
            let start = state["reconcile_started"].as_str().unwrap_or(started);
            let page = self
                .source
                .issues(
                    canonical,
                    "all",
                    state["reconcile_page"].as_u64().unwrap_or(1),
                    None,
                    "created",
                )
                .await?;
            let pulls = self.enrich(canonical, &page.items).await?;
            self.store.save_items(&repo, &page.items, &pulls).await?;
            self.store.update_poll(key,if let Some(next)=page.next{json!({"reconcile_page":next,"reconcile_started":start})}else{json!({"reconcile_page":null,"reconcile_started":null,"last_reconcile":started})}).await?;
            if page.next.is_none() {
                self.store
                    .execute(
                        "UPDATE items SET unavailable=1 WHERE repo_id=? AND last_seen<?",
                        &[json!(key), json!(start)],
                    )
                    .await?;
            }
        }
        self.store
            .update_poll(key, json!({"error":null,"next_retry":null,"failures":0,"active":0}))
            .await?;
        Ok(())
    }
    pub async fn engagement(
        &mut self,
        limit: usize,
        force: bool,
        repo: Option<&str>,
    ) -> Result<()> {
        for item in self
            .store
            .candidates(limit, if force { 0 } else { self.interval }, repo)
            .await?
        {
            if self.cancelled() {
                break;
            }
            if self.ensure_active(string(&item, "repo")).await.is_err() {
                continue;
            }
            match self
                .source
                .engagement(
                    string(&item, "repo"),
                    item["number"].as_u64().unwrap_or_default(),
                    string(&item, "type"),
                )
                .await
            {
                Ok(events) => {
                    self.store
                        .save_engagement(string(&item, "id"), &events, string(&item, "updated_at"))
                        .await?
                }
                Err(error) => {
                    let gh = error.downcast_ref::<GitHubError>();
                    let retry = gh
                        .map(|e| e.retry_at)
                        .filter(|t| *t > 0)
                        .unwrap_or_else(|| Utc::now().timestamp_millis() + 300000);
                    let message = gh
                        .map(|e| e.message.as_str())
                        .unwrap_or("Discussion import interrupted; previous snapshot retained");
                    self.store
                        .engagement_failure(
                            string(&item, "id"),
                            message,
                            &DateTime::from_timestamp_millis(retry).unwrap().to_rfc3339(),
                        )
                        .await?;
                    if gh.is_some_and(|e| matches!(e.status, 401 | 429)) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn tick(&mut self) -> Result<()> {
        self.engagement(8, false, None).await?;
        if self.cancelled() {
            return Ok(());
        }
        let repos = self.store.repositories().await?;
        if repos.is_empty() {
            return Ok(());
        }
        let repo = &repos[self.cursor % repos.len()];
        self.cursor = (self.cursor + 1) % repos.len();
        self.step(string(repo, "configured_name"), false).await
    }
    pub async fn hard_refresh(&mut self, scope: Option<String>) -> Result<()> {
        self.hard_refresh_repositories(scope.map(|repo| vec![repo]))
            .await
    }
    pub async fn hard_refresh_repositories(&mut self, scope: Option<Vec<String>>) -> Result<()> {
        let result = self.refresh_inner(scope.as_deref()).await;
        let mut status = self.status.lock().unwrap();
        status.status = if result.is_ok() { "complete" } else { "failed" }.into();
        status.message = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "GitHub refresh complete".into());
        result
    }
    async fn refresh_inner(&mut self, scope: Option<&[String]>) -> Result<()> {
        let repos = self
            .store
            .repositories()
            .await?
            .into_iter()
            .filter(|r| {
                scope.is_none_or(|repos| {
                    repos
                        .iter()
                        .any(|s| string(r, "name") == s || string(r, "configured_name") == s)
                })
            })
            .collect::<Vec<_>>();
        if repos.is_empty() {
            anyhow::bail!("No matching repositories configured");
        }
        let mut errors = vec![];
        for (index, repo) in repos.iter().enumerate() {
            if self.cancelled() {
                anyhow::bail!("Refresh interrupted by shutdown");
            }
            let name = string(repo, "configured_name");
            if self.ensure_active(name).await.is_err() {
                continue;
            }
            let state = self.store.poll_state(string(repo, "id")).await?;
            if self.paused.contains(name) && state["error"].is_null() {
                self.paused.remove(name);
                self.metadata.remove(name);
            }
            if self.paused.contains(name) || retry_future(&state["next_retry"]) {
                errors.push(format!(
                    "{}: waiting for retry or authentication",
                    string(repo, "name")
                ));
                continue;
            }
            self.progress(format!(
                "Checking {} ({}/{})…",
                string(repo, "name"),
                index + 1,
                repos.len()
            ));
            loop {
                if self.ensure_active(name).await.is_err() {
                    break;
                }
                self.step(name, true).await?;
                let current = self.store.repository(name).await?;
                let state = self.store.poll_state(string(&current, "id")).await?;
                if !state["error"].is_null() {
                    errors.push(format!("{}: {}", name, string(&state, "error")));
                    break;
                }
                if self.cancelled() || state["stage"] != "opening" {
                    break;
                }
            }
            let current = self.store.repository(name).await?;
            self.progress(format!(
                "Refreshing discussion in {} ({}/{})…",
                string(&current, "name"),
                index + 1,
                repos.len()
            ));
            self.engagement(1_000_000, true, Some(string(&current, "name")))
                .await?;
            let errors_in_repo=self.store.one("SELECT count(*) AS n FROM engagement_sync es JOIN items i ON i.id=es.id WHERE i.repo_id=? AND i.state IN ('open','draft') AND es.error IS NOT NULL",&[current["id"].clone()]).await?;
            if errors_in_repo["n"].as_u64().unwrap_or(0) > 0 {
                errors.push(format!("{}: some discussion imports need a retry", name));
            }
        }
        if self.cancelled() {
            anyhow::bail!("Refresh interrupted by shutdown");
        }
        if !errors.is_empty() {
            anyhow::bail!("Refresh incomplete: {}", errors.join("; "));
        }
        Ok(())
    }
}

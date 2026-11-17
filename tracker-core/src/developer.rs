//! Shared imported items drive developer queues; search only discovers participation.
use crate::{
    github::{SharedGitHub, Source},
    store::Store,
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

pub const REASONS: [(&str, &str); 5] = [
    ("review", "review-requested"),
    ("assigned", "assignee"),
    ("author", "author"),
    ("mentioned", "mentions"),
    ("commented", "commenter"),
];
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub reasons: Vec<String>,
    #[serde(default)]
    pub since: String,
    #[serde(default)]
    pub action_times: BTreeMap<String, String>,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub times_known: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct RepoCache {
    #[serde(default)]
    fetched: i64,
    #[serde(default)]
    members: Vec<Member>,
    #[serde(default)]
    categories: BTreeMap<String, Category>,
}
#[derive(Default, Serialize, Deserialize)]
struct Category {
    fetched: i64,
    members: Vec<Member>,
    #[serde(default)]
    retry_at: i64,
    #[serde(default)]
    error: Option<String>,
}
pub fn required(queue: &str) -> &'static [&'static str] {
    match queue {
        "cached" => &[],
        "review" => &["review"],
        "assigned" => &["assigned"],
        "created" => &["author"],
        "participating" => &["author", "mentioned", "commented"],
        _ => &["review", "assigned", "author", "mentioned", "commented"],
    }
}
fn write_cache(path: &Path, cache: &impl Serialize) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec(cache)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    repositories: BTreeMap<String, RepoCache>,
}

pub fn validate_login(login: &str) -> Result<String> {
    let login = login.trim().to_lowercase();
    if login.is_empty()
        || login.len() > 39
        || !login
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        bail!("Choose a valid GitHub maintainer login");
    }
    Ok(login)
}
pub fn query(repo: &str, login: &str, qualifier: &str, closed: bool) -> String {
    format!(
        "repo:{repo} {qualifier}:{login}{}{}",
        if qualifier == "review-requested" {
            " is:pr"
        } else {
            ""
        },
        if closed && qualifier != "review-requested" {
            ""
        } else {
            " is:open"
        }
    )
}

pub async fn load(
    directory: &Path,
    store: &Store,
    github: Option<SharedGitHub>,
    login: &str,
    closed: bool,
    force: bool,
) -> Result<Value> {
    load_progress(
        directory,
        store,
        github,
        login,
        closed,
        force,
        "all",
        &|_| {},
    )
    .await
}

pub async fn load_progress(
    directory: &Path,
    store: &Store,
    github: Option<SharedGitHub>,
    login: &str,
    closed: bool,
    force: bool,
    queue: &str,
    progress: &(dyn Fn(String) + Send + Sync),
) -> Result<Value> {
    let login = validate_login(login)?;
    let path = directory.join("developer-work").join(format!(
        "{login}-{}.json",
        if closed { "history" } else { "open" }
    ));
    let mut cache: Cache = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let repos = store.repositories().await?;
    let now = chrono::Utc::now().timestamp_millis();
    let pause_path = directory.join("developer-work/cooldown.json");
    let mut pause = std::fs::read(&pause_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<i64>(&b).ok())
        .unwrap_or(0);
    if let Some(g) = &github {
        pause = pause.max(g.0.lock().await.pause_until());
    }
    if pause > now {
        write_cache(&pause_path, &pause)?;
        if let Some(g) = &github {
            g.0.lock().await.defer_until(pause);
        }
    }
    let mut warnings = Vec::new();
    let mut merged: BTreeMap<String, Member> = BTreeMap::new();
    let mut coverage: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut retry_at = pause;
    for (index, repo) in repos.iter().enumerate() {
        let name = repo["name"].as_str().unwrap_or_default();
        let saved = cache.repositories.entry(name.into()).or_default();
        // Upgrade the previous all-queues cache without discarding useful saved results.
        if saved.categories.is_empty() && saved.fetched > 0 {
            for (reason, _) in REASONS {
                saved.categories.insert(
                    reason.into(),
                    Category {
                        fetched: saved.fetched,
                        members: saved
                            .members
                            .iter()
                            .filter(|m| m.reasons.iter().any(|r| r == reason))
                            .cloned()
                            .map(|mut m| {
                                m.reasons = vec![reason.into()];
                                m
                            })
                            .collect(),
                        ..Default::default()
                    },
                );
            }
        }
        // Author and assignment membership always comes from the shared importer.
        // Review membership is shared across profiles too, with REST only for missing versions.
        for reason in required(queue).iter().filter(|r| **r != "author") {
            let category = saved.categories.entry((*reason).into()).or_default();
            let fresh = now / 1000 - category.fetched < 900;
            if pause <= now
                && category.retry_at <= now
                && (force || !fresh || matches!(*reason, "review" | "assigned"))
            {
                progress(format!(
                    "{login} · {name} · {reason} · repository {} of {}",
                    index + 1,
                    repos.len()
                ));
                let result = match &github {
                    Some(g) => {
                        fetch(
                            store,
                            g.clone(),
                            repo,
                            &login,
                            closed,
                            reason,
                            &category.members,
                        )
                        .await
                    }
                    None if matches!(*reason, "review" | "assigned") => Ok(local_members(
                        &store.developer_items(name, closed).await?,
                        &login,
                        reason,
                        &category.members,
                    )),
                    None => Err(anyhow::anyhow!("GitHub disconnected")),
                };
                match result {
                    Ok(members) => {
                        *category = Category {
                            fetched: chrono::Utc::now().timestamp(),
                            members,
                            ..Default::default()
                        };
                    }
                    Err(e) => {
                        let deadline = e
                            .downcast_ref::<crate::github::GitHubError>()
                            .map(|e| e.retry_at)
                            .unwrap_or(0)
                            .max(chrono::Utc::now().timestamp_millis() + 300_000);
                        category.retry_at = deadline;
                        category.error = Some(e.to_string());
                        retry_at = retry_at.max(deadline);
                        if e.downcast_ref::<crate::github::GitHubError>()
                            .is_some_and(|e| e.status == 429)
                        {
                            pause = deadline;
                            write_cache(&pause_path, &pause)?;
                            if let Some(g) = &github {
                                g.0.lock().await.defer_until(pause);
                            }
                        }
                    }
                }
            }
            if category.retry_at > now {
                retry_at = retry_at.max(category.retry_at);
                if pause <= now {
                    if let Some(error) = &category.error {
                        warnings.push(format!("{name}: {error}"));
                    }
                }
            }
        }
        let local = store.developer_items(name, closed).await?;
        let state = store
            .poll_state(repo["id"].as_str().unwrap_or_default())
            .await?;
        let complete = if closed {
            state["stage"] == "ready"
        } else {
            matches!(state["stage"].as_str(), Some("backfill" | "ready"))
        };
        let checked = state["last_success"]
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.timestamp())
            .unwrap_or(0);
        for reason in ["author", "assigned", "review"] {
            let old = saved.categories.entry(reason.into()).or_default();
            let members = local_members(&local, &login, reason, &old.members);
            let reviewers_complete = local
                .iter()
                .filter(|i| {
                    i["type"] == "pr" && matches!(i["state"].as_str(), Some("open" | "draft"))
                })
                .all(|i| {
                    i["reviewer_version"].is_string() && i["reviewer_version"] == i["updated_at"]
                });
            old.members = members;
            old.fetched = if complete && (reason != "review" || reviewers_complete) {
                checked
            } else {
                0
            };
            if reason == "author" {
                old.retry_at = 0;
                old.error = None;
            }
        }
        // Known discussion participants remain visible even before discovery search succeeds.
        let events=store.query("SELECT DISTINCT e.item_id AS id FROM engagement_events e JOIN authors a ON a.id=e.author_id JOIN items i ON i.id=e.item_id WHERE i.repo_id=? AND lower(a.login)=?",&[repo["id"].clone(),json!(login)]).await?;
        let mut participants: std::collections::HashSet<String> = events
            .iter()
            .filter_map(|e| e["id"].as_str().map(str::to_owned))
            .collect();
        participants.extend(cached_participants(directory, name, &local, &login));
        for item in &local {
            if participants.contains(item["id"].as_str().unwrap_or_default()) {
                let m = member(item, "commented");
                merged
                    .entry(m.id.clone())
                    .or_insert(m)
                    .reasons
                    .push("commented".into());
            }
        }
        for (reason, category) in &saved.categories {
            if category.fetched > 0 {
                coverage
                    .entry(reason.clone())
                    .or_default()
                    .push(category.fetched);
            }
            for member in &category.members {
                let entry = merged.entry(member.id.clone()).or_insert_with(|| {
                    let mut m = member.clone();
                    m.reasons.clear();
                    m
                });
                if !entry.reasons.contains(reason) {
                    entry.reasons.push(reason.clone());
                }
                entry.action_times.extend(member.action_times.clone());
            }
        }
    }
    write_cache(&path, &cache)?;
    let loaded_queues: Vec<&str> = ["review", "assigned", "created", "participating"]
        .into_iter()
        .filter(|q| {
            required(q).iter().all(|r| {
                coverage
                    .get(*r)
                    .map_or(repos.is_empty(), |v| v.len() == repos.len())
            })
        })
        .collect();
    let checked = if required(queue).iter().all(|r| {
        coverage
            .get(*r)
            .map_or(repos.is_empty(), |v| v.len() == repos.len())
    }) {
        required(queue)
            .iter()
            .filter_map(|r| coverage.get(*r))
            .flatten()
            .copied()
            .min()
    } else {
        None
    };
    let mut items = Vec::new();
    for member in merged.into_values() {
        if let Ok(mut item) = store.detail(&member.id).await {
            let closed_item = matches!(item["state"].as_str(), Some("closed" | "merged"));
            if !closed && closed_item {
                continue;
            }
            let mut reasons = member.reasons;
            if closed_item {
                reasons.retain(|r| r != "review");
            }
            if reasons.is_empty() {
                continue;
            }
            reasons.sort();
            reasons.dedup();
            item["developerReasons"] = json!(reasons);
            item["actionSince"] = json!(member.since);
            item["actionTimes"] = json!(member.action_times);
            items.push(item);
        }
    }
    let warning = if pause > now {
        Some("Showing saved results where available. Unloaded queues are marked —. Navigation and Refresh will respect the cooldown.".to_owned())
    } else if let Some(first) = warnings.first() {
        Some(format!(
            "{first}. {} request(s) delayed; saved results retained.",
            warnings.len()
        ))
    } else if !loaded_queues.contains(&queue) && !matches!(queue, "all" | "cached") {
        Some(
            "Shared repository sync is still loading this queue. Available items are shown now."
                .into(),
        )
    } else {
        None
    };
    Ok(
        json!({"login":login,"items":items,"warning":warning,"checked":checked,"retry_at":if retry_at>now{Some(retry_at)}else{None},"paused":pause>now,"loaded_queues":loaded_queues}),
    )
}

async fn fetch(
    store: &Store,
    github: SharedGitHub,
    repo: &Value,
    login: &str,
    closed: bool,
    only: &str,
    previous: &[Member],
) -> Result<Vec<Member>> {
    let name = repo["name"].as_str().unwrap_or_default();
    if matches!(only, "review" | "assigned") {
        let items = store.developer_items(name, closed).await?;
        if only == "review" {
            for item in &items {
                if item["type"] != "pr"
                    || !matches!(item["state"].as_str(), Some("open" | "draft"))
                    || (item["reviewer_version"].is_string()
                        && item["reviewer_version"] == item["updated_at"])
                {
                    continue;
                }
                let pull = github
                    .0
                    .lock()
                    .await
                    .pull_details(name, item["number"].as_u64().unwrap())
                    .await?;
                store
                    .save_reviewers(
                        item["id"].as_str().unwrap(),
                        item["updated_at"].as_str().unwrap_or_default(),
                        &pull["requested_reviewers"],
                    )
                    .await?;
            }
        }
        let items = store.developer_items(name, closed).await?;
        let mut members = local_members(&items, login, only, previous);
        for member in &mut members {
            if previous.iter().any(|p| {
                p.id == member.id
                    && p.updated == member.updated
                    && (p.times_known || !p.action_times.is_empty())
            }) {
                continue;
            }
            let item = items.iter().find(|i| i["id"] == member.id).unwrap();
            if let Some(times) = store
                .action_times(&member.id, login, only, &member.updated)
                .await?
            {
                member.action_times = times;
            } else {
                member.action_times = github
                    .0
                    .lock()
                    .await
                    .action_since(
                        name,
                        item["number"].as_u64().unwrap(),
                        login,
                        &member.reasons,
                    )
                    .await?;
                store
                    .save_action_times(
                        &member.id,
                        login,
                        only,
                        &member.updated,
                        &member.action_times,
                    )
                    .await?;
            }
            member.times_known = true;
        }
        return Ok(members);
    }
    // Reuse resolved metadata, including resolution by another queue or the importer.
    let current = store.repository(name).await?;
    let repo = &current;
    // Resolve the repository before storing items so they share the normal stable IDs.
    let repo = if repo["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("pending:"))
    {
        let meta = github.clone().repository(name).await?;
        store.save_repository(name, &meta).await?
    } else {
        repo.clone()
    };
    let mut entries: BTreeMap<String, (Value, Member)> = BTreeMap::new();
    for (reason, qualifier) in REASONS.into_iter().filter(|r| r.0 == only) {
        let q = query(name, login, qualifier, closed);
        let mut page = 1;
        loop {
            let response = github.0.lock().await.search_issues(&q, page).await?;
            for item in response.items {
                let id = item["id"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Invalid search item"))?
                    .to_string();
                let entry = entries.entry(id.clone()).or_insert_with(|| {
                    (
                        item.clone(),
                        Member {
                            id,
                            reasons: vec![],
                            since: String::new(),
                            action_times: BTreeMap::new(),
                            updated: item["updated_at"].as_str().unwrap_or_default().into(),
                            times_known: false,
                        },
                    )
                });
                if !entry.1.reasons.iter().any(|r| r == reason) {
                    entry.1.reasons.push(reason.into());
                }
            }
            if let Some(next) = response.next {
                page = next;
            } else {
                break;
            }
        }
    }
    let raw: Vec<Value> = entries.values().map(|e| e.0.clone()).collect();
    store.save_items(&repo, &raw, &HashMap::new()).await?;
    for (item, member) in entries.values_mut() {
        member.since = item["created_at"].as_str().unwrap_or_default().into();
    }
    Ok(entries
        .into_values()
        .map(|e| e.1)
        .filter(|m| !m.reasons.is_empty())
        .collect())
}

fn member(item: &Value, reason: &str) -> Member {
    Member {
        id: item["id"].as_str().unwrap_or_default().into(),
        reasons: vec![reason.into()],
        since: item["created_at"].as_str().unwrap_or_default().into(),
        updated: item["updated_at"].as_str().unwrap_or_default().into(),
        ..Default::default()
    }
}
fn local_members(items: &[Value], login: &str, reason: &str, previous: &[Member]) -> Vec<Member> {
    items
        .iter()
        .filter(|item| {
            let matches = |v: &Value| v.as_str().is_some_and(|v| v.eq_ignore_ascii_case(login));
            match reason {
                "author" => matches(&item["login"]),
                "assigned" => {
                    serde_json::from_str::<Vec<Value>>(item["assignees"].as_str().unwrap_or("[]"))
                        .unwrap_or_default()
                        .iter()
                        .any(|u| matches(&u["login"]))
                }
                "review" => {
                    matches!(item["state"].as_str(), Some("open" | "draft"))
                        && serde_json::from_str::<Vec<Value>>(
                            item["reviewers"].as_str().unwrap_or("[]"),
                        )
                        .unwrap_or_default()
                        .iter()
                        .any(|u| matches(&u["login"]))
                }
                _ => false,
            }
        })
        .map(|item| {
            let mut m = member(item, reason);
            if let Some(old) = previous
                .iter()
                .find(|v| v.id == m.id && v.updated == m.updated)
            {
                m.action_times = old.action_times.clone();
                m.times_known = old.times_known;
            }
            m
        })
        .collect()
}
fn cached_participants(directory: &Path, repo: &str, items: &[Value], login: &str) -> Vec<String> {
    let folder = repo
        .to_lowercase()
        .split('/')
        .fold(directory.join("discussions"), |p, c| {
            p.join(format!("repo-{c}"))
        });
    let by_number: HashMap<u64, &Value> = items
        .iter()
        .filter_map(|i| i["number"].as_u64().map(|n| (n, i)))
        .collect();
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with("-reviews-v1.json") {
                return None;
            }
            let number = name.split('-').next()?.parse::<u64>().ok()?;
            let item = by_number.get(&number)?;
            let cache: Value = serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
            if cache["version"] != item["updated_at"] {
                return None;
            }
            cache["data"]["comments"]
                .as_array()?
                .iter()
                .any(|c| {
                    c["author"]
                        .as_str()
                        .is_some_and(|a| a.eq_ignore_ascii_case(login))
                })
                .then(|| item["id"].as_str().unwrap_or_default().into())
        })
        .collect()
}

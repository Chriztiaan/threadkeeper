use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock};
use tracker_core::{Backend,BackendOptions};
pub use tracker_core::poller::RefreshStatus;

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Person { #[serde(rename="vip",alias="customer")] pub customer:bool, #[serde(rename="fullName")] pub full_name:String, pub notes:String, pub company:String, pub highlight:String, pub login: String, pub internal: bool, pub customers: Vec<String>, pub maintainer: bool }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Label { pub name: String, pub color: Option<String> }
#[derive(Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Advice { pub text: String, pub enabled: bool, #[serde(rename="updatedAt")] pub updated_at: Option<String> }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Engagement { pub at: String, pub kind: String, pub author: Option<Person>, pub pending: bool, pub stale: bool, pub error: Option<String> }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Related { pub id: Option<String>, pub repo: String, pub number: u64, pub title: Option<String>, pub url: String, pub kind: String }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Item {
    #[serde(rename="developerReasons")] pub developer_reasons:Vec<String>, #[serde(rename="actionSince")] pub action_since:String, #[serde(rename="actionTimes")] pub action_times:std::collections::BTreeMap<String,String>,
    pub id: String, pub repo: String, pub number: u64, pub title: String,
    #[serde(rename="type")] pub kind: String, pub state: String, pub author: Option<Person>,
    pub labels: Vec<Label>, pub assignees: Vec<Person>, pub comments: u64,
    pub url: String, pub created_at: String, pub updated_at: String, pub body: String,
    pub engagement: Engagement, pub advice: Advice, pub related: Vec<Related>,
}
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Pen { pub id: String, pub label: String, pub total: usize, pub items: Vec<Item>, #[serde(rename="hasMore")] pub has_more: bool }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Pending { pub total: usize, pub items: Vec<Item> }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Sync { pub complete: bool, pub pending: usize, pub errors: usize, pub stale: usize, #[serde(rename="oldestCheckedAt")] pub oldest: Option<String> }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Snapshot { #[serde(rename="vipLogins")] pub vip_logins:Vec<String>, #[serde(rename="repositorySync")] pub repository_sync:Vec<RepositorySync>, pub backend: BackendStatus, pub repositories: Vec<String>, pub total: usize, pub pens: Vec<Pen>, pub pending: Pending, pub sync: Sync, #[serde(rename="maintainerCount")] pub maintainers: usize }
impl Snapshot {
    pub fn items(&self) -> Vec<Item> { self.pens.iter().flat_map(|p| p.items.clone()).chain(self.pending.items.clone()).collect() }
}
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BackendStatus {pub developer_progress:Option<String>,pub auth_failed:bool,pub polling:bool,pub message:String,pub config_error:Option<String>}
static BACKEND:OnceLock<Result<Backend,String>>=OnceLock::new();
static SESSION_TOKEN:OnceLock<String>=OnceLock::new();
pub fn authorized()->bool{SESSION_TOKEN.get().is_some()}
pub fn authorize(token:String){let _=SESSION_TOKEN.set(token);}
fn backend()->Result<&'static Backend,String>{
    let mut options=BackendOptions::default();
    let inspection=std::env::args().any(|arg|arg=="--check-backend");
    if !authorized()&&!inspection{return Err("Connect GitHub before opening the tracker.".into());}
    if authorized(){options.token=SESSION_TOKEN.get().cloned();options.discover_credentials=false;}
    BACKEND.get_or_init(||Backend::start(options).map_err(|e|e.to_string())).as_ref().map_err(Clone::clone)
}
pub fn start(wake:impl Fn()+Send+std::marker::Sync+'static){if let Ok(backend)=backend(){backend.set_waker(wake);}}
pub fn shutdown(){if let Some(Ok(backend))=BACKEND.get(){backend.shutdown();}}
pub fn revision()->u64{BACKEND.get().and_then(|r|r.as_ref().ok()).map(Backend::revision).unwrap_or(0)}
pub fn cache_path(name:&str)->PathBuf {
    let directory=tracker_core::config::data_dir();
    let file=directory.join(if name=="desktop-followup.json" {"snapshot.json"} else {name});
    if file.exists(){file}else{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local").join(name)}
}
pub fn github_refresh(repos:Vec<String>)->Result<RefreshStatus,String>{backend()?.refresh_repositories(repos).map_err(|e|e.to_string())}
pub fn refresh_status()->Result<RefreshStatus,String>{backend()?.refresh_status().map_err(|e|e.to_string())}
pub fn snapshot()->Result<Snapshot,String>{serde_json::from_value(backend()?.snapshot().map_err(|e|e.to_string())?).map_err(|e|format!("Could not read tracker snapshot: {e}"))}
pub fn detail(id:&str)->Result<Item,String>{serde_json::from_value(backend()?.detail(id.into()).map_err(|e|e.to_string())?).map_err(|e|format!("Could not read entry: {e}"))}
pub fn save_advice(id:&str,text:&str,enabled:bool)->Result<Advice,String>{serde_json::from_value(backend()?.advice(id.into(),text.into(),enabled).map_err(|e|e.to_string())?).map_err(|e|format!("Could not read advice: {e}"))}
pub fn age(at: &str) -> String {
    age_at(at,Utc::now())
}
pub fn age_at(at:&str,now:DateTime<Utc>)->String {
    let Ok(at) = DateTime::parse_from_rfc3339(at) else { return "—".into() };
    let minutes = (now - at.with_timezone(&Utc)).num_minutes().max(0);
    if minutes < 1 { "just now".into() } else if minutes < 60 { format!("{minutes}m") }
    else if minutes < 1440 { format!("{}h", minutes / 60) } else { format!("{}d", minutes / 1440) }
}
pub fn date(at: &str) -> String { DateTime::parse_from_rfc3339(at).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_else(|_| "—".into()) }
pub fn repo_name(repo: &str) -> &str { repo.split('/').next_back().unwrap_or(repo) }
pub fn pen_for(data: &Snapshot, id: &str) -> String { data.pens.iter().find(|p| p.items.iter().any(|i| i.id == id)).map(|p| p.label.clone()).unwrap_or_else(|| "Awaiting sync".into()) }

pub fn repository_settings()->Result<tracker_core::repository_config::RepositorySettings,String>{backend()?.repository_settings().map_err(|e|e.to_string())}
pub fn save_repositories(repos:Vec<tracker_core::config::Repository>,revision:String)->Result<tracker_core::repository_config::RepositorySettings,String>{backend()?.save_repositories(repos,revision).map_err(|e|e.to_string())}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Comment {pub kind:String,pub review_state:String, pub id: u64, pub author: String, pub body: String, pub created_at: String, pub updated_at: String, pub url: String }
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Discussion { pub comments: Vec<Comment>, pub next: Option<u64>, pub warning: Option<String> }
pub fn discussion(id:&str,page:u64,force:bool)->Result<Discussion,String>{
    serde_json::from_value(backend()?.discussion(id.into(),page,force).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}

pub fn maintainer_settings()->Result<tracker_core::maintainer_config::MaintainerSettings,String>{backend()?.maintainer_settings().map_err(|e|e.to_string())}
pub fn save_maintainers(people:Vec<tracker_core::config::Maintainer>,revision:String)->Result<tracker_core::maintainer_config::MaintainerSettings,String>{backend()?.save_maintainers(people,revision).map_err(|e|e.to_string())}

pub fn customer_settings()->Result<tracker_core::customer_config::CustomerSettings,String>{backend()?.customer_settings().map_err(|e|e.to_string())}
pub fn save_customers(people:Vec<tracker_core::customer_config::Customer>,revision:String)->Result<tracker_core::customer_config::CustomerSettings,String>{backend()?.save_customers(people,revision).map_err(|e|e.to_string())}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RepositorySync {
    pub repo:String, pub status:String, pub stage:String, pub last_success:Option<String>,
    pub error:Option<String>,pub next_retry:Option<String>,pub open_page:Option<u64>,pub backfill_page:Option<u64>,pub pending:usize,pub matched:usize,
}
impl RepositorySync {
    pub fn label(&self,polling:bool)->String {
        if !polling {return "Polling paused · sign in and reopen".into();}
        match self.status.as_str() {
            "queued"=>"Queued for initial import".into(),
            "waiting"=>if self.stage=="backfill"{format!("Waiting for next history page · page {}",self.backfill_page.unwrap_or(1))}else{format!("Waiting for next open-entry page · page {}",self.open_page.unwrap_or(1))},
            "importing"=>if self.stage=="backfill"{format!("Importing history · page {}",self.backfill_page.unwrap_or(1))}else{format!("Importing open entries · page {}",self.open_page.unwrap_or(1))},
            "polling"=>"Checking for updates…".into(),
            "syncing"=>format!("Syncing discussions · {} pending",self.pending),
            "error"=>format!("{} · {}",if self.next_retry.is_some(){"Retry scheduled"}else{"Needs attention"},self.error.as_deref().unwrap_or("Sync failed")),
            "ready"=>{let checked=self.last_success.as_deref().map(|at|{let elapsed=age(at);if elapsed=="just now"{elapsed}else{format!("{elapsed} ago")}}).unwrap_or_else(||"—".into());format!("Up to date · checked {checked} · {} open matches",self.matched)},
            _=>"Waiting for sync status…".into(),
        }
    }
}

pub fn replace_token(token:String)->Result<(),String>{backend()?.replace_token(token).map_err(|e|e.to_string())?;backend()?.refresh(None).map_err(|e|e.to_string())?;Ok(())}

#[derive(Clone,Default,serde::Deserialize)]
#[serde(default)]
pub struct DeveloperResult {pub retry_at:Option<i64>,pub paused:bool,pub loaded_queues:Vec<String>,pub login:String,pub items:Vec<Item>,pub warning:Option<String>,pub checked:Option<i64>}
pub fn developer_work(login:String,closed:bool,force:bool,queue:String)->Result<DeveloperResult,String>{serde_json::from_value(backend()?.developer_work(login,closed,force,queue).map_err(|e|e.to_string())?).map_err(|e|e.to_string())}

use crate::{
    config::Repository,
    config::{self, Config},
    github::{GitHub, SharedGitHub},
    poller::{Poller, RefreshStatus, Status},
    repository_config::{RepositoryManager, RepositorySettings},
    store::Store,
};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use futures_lite::StreamExt;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::{Notify, mpsc as channel};

#[derive(Clone)]
pub struct BackendOptions {
    pub directory: PathBuf,
    pub legacy: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub token: Option<String>,
    pub discover_credentials: bool,
    pub polling: bool,
}
impl Default for BackendOptions {
    fn default() -> Self {
        Self {
            directory: config::data_dir(),
            legacy: std::env::var_os("TRACKER_IMPORT_DB")
                .map(PathBuf::from)
                .or_else(|| {
                    Some(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .unwrap()
                            .join("data/tracker.sqlite"),
                    )
                }),
            config: std::env::var_os("TRACKER_CONFIG").map(PathBuf::from),
            token: None,
            discover_credentials: true,
            polling: std::env::var("TRACKER_GITHUB_POLLING").as_deref() != Ok("0"),
        }
    }
}
enum Query {
    ReplaceToken(String),
    Developer(String,bool,bool,String),
    Snapshot,
    Repositories,
    Customers,
    SaveCustomers(Vec<crate::customer_config::Customer>,String),
    Maintainers,
    SaveMaintainers(Vec<crate::config::Maintainer>,String),
    SaveRepositories(Vec<Repository>, String),
    Detail(String),
    Discussion(String, u64, bool),
    Advice(String, String, bool),
}
struct Request {
    query: Query,
    reply: mpsc::Sender<Result<Value, String>>,
}
struct Inner {
    requests: channel::UnboundedSender<Request>,
    manual: channel::UnboundedSender<Option<Vec<String>>>,
    status: Status,
    stopped: Arc<AtomicBool>,
    stop: Arc<Notify>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    error: Mutex<Option<String>>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    revision: AtomicU64,
    developer_progress:Mutex<Option<String>>,
}
#[derive(Clone)]
pub struct Backend {
    inner: Arc<Inner>,
}
impl Backend {
    pub fn start(options: BackendOptions) -> Result<Self> {
        let (requests, rx) = channel::unbounded_channel();
        let (manual, mrx) = channel::unbounded_channel();
        let inner = Arc::new(Inner {
            requests,
            manual,
            status: Arc::new(Mutex::new(RefreshStatus::default())),
            stopped: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(Notify::new()),
            thread: Mutex::new(None),
            error: Mutex::new(None),
            wake: Mutex::new(None),
            revision: AtomicU64::new(0),
            developer_progress:Mutex::new(None),
        });
        let state = inner.clone();
        let thread = std::thread::Builder::new()
            .name("powersync-tracker".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .enable_all()
                        .build()?;
                    runtime.block_on(run(options, state.clone(), rx, mrx))
                })();
                if let Err(error) = result {
                    *state.error.lock().unwrap() = Some(format!("Local tracker: {error:#}"));
                    state.revision.fetch_add(1, Ordering::Relaxed);
                    if let Some(wake) = state.wake.lock().unwrap().as_ref() {
                        wake();
                    }
                }
            })?;
        *inner.thread.lock().unwrap() = Some(thread);
        Ok(Self { inner })
    }
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self.inner.wake.lock().unwrap() = Some(Arc::new(wake));
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Relaxed)
    }
    fn request(&self, query: Query) -> Result<Value> {
        if let Some(error) = self.inner.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        let timeout = if matches!(&query, Query::Developer(..) | Query::SaveRepositories(..) | Query::SaveMaintainers(..) | Query::SaveCustomers(..) | Query::Discussion(..)) {
            Duration::from_secs(900)
        } else {
            Duration::from_secs(60)
        };
        let (tx, rx) = mpsc::channel();
        self.inner
            .requests
            .send(Request { query, reply: tx })
            .map_err(|_| anyhow::anyhow!("Local worker unavailable"))?;
        rx.recv_timeout(timeout)
            .map_err(|_| {
                anyhow::anyhow!(
                    self.inner
                        .error
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(|| "Local worker did not respond".into())
                )
            })?
            .map_err(anyhow::Error::msg)
    }
    pub fn snapshot(&self) -> Result<Value> {
        self.request(Query::Snapshot)
    }
    pub fn customer_settings(&self)->Result<crate::customer_config::CustomerSettings>{Ok(serde_json::from_value(self.request(Query::Customers)?)?)}
    pub fn save_customers(&self,customers:Vec<crate::customer_config::Customer>,revision:String)->Result<crate::customer_config::CustomerSettings>{Ok(serde_json::from_value(self.request(Query::SaveCustomers(customers,revision))?)?)}
    pub fn maintainer_settings(&self)->Result<crate::maintainer_config::MaintainerSettings>{Ok(serde_json::from_value(self.request(Query::Maintainers)?)?)}
    pub fn save_maintainers(&self,maintainers:Vec<crate::config::Maintainer>,revision:String)->Result<crate::maintainer_config::MaintainerSettings>{Ok(serde_json::from_value(self.request(Query::SaveMaintainers(maintainers,revision))?)?)}
    pub fn repository_settings(&self) -> Result<RepositorySettings> {
        Ok(serde_json::from_value(self.request(Query::Repositories)?)?)
    }
    pub fn save_repositories(
        &self,
        repositories: Vec<Repository>,
        revision: String,
    ) -> Result<RepositorySettings> {
        Ok(serde_json::from_value(self.request(
            Query::SaveRepositories(repositories, revision),
        )?)?)
    }
    pub fn detail(&self, key: String) -> Result<Value> {
        self.request(Query::Detail(key))
    }
    pub fn discussion(&self, key: String, page: u64, force: bool) -> Result<Value> {
        self.request(Query::Discussion(key, page, force))
    }
    pub fn advice(&self, key: String, text: String, enabled: bool) -> Result<Value> {
        self.request(Query::Advice(key, text, enabled))
    }
    pub fn refresh(&self, repo: Option<String>) -> Result<RefreshStatus> {
        self.refresh_scope(repo.map(|repo| vec![repo]))
    }
    pub fn refresh_repositories(&self, repos: Vec<String>) -> Result<RefreshStatus> {
        self.refresh_scope(Some(repos))
    }
    fn refresh_scope(&self, repos: Option<Vec<String>>) -> Result<RefreshStatus> {
        if let Some(error) = self.inner.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        let mut status = self.inner.status.lock().unwrap();
        if status.status == "running" {
            return Ok(status.clone());
        }
        *status = RefreshStatus {
            status: "running".into(),
            message: "Waiting for the current GitHub request…".into(),
            repo: repos
                .as_ref()
                .filter(|repos| repos.len() == 1)
                .map(|repos| repos[0].clone()),
            repositories: repos.clone(),
        };
        self.inner
            .manual
            .send(repos)
            .map_err(|_| anyhow::anyhow!("Local worker unavailable"))?;
        Ok(status.clone())
    }
    pub fn developer_work(&self,login:String,closed:bool,force:bool,queue:String)->Result<Value>{self.request(Query::Developer(login,closed,force,queue))}
    pub fn replace_token(&self, token:String)->Result<()> {self.request(Query::ReplaceToken(token)).map(|_|())}
    pub fn refresh_status(&self) -> Result<RefreshStatus> {
        if let Some(error) = self.inner.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        Ok(self.inner.status.lock().unwrap().clone())
    }
    pub fn shutdown(&self) {
        self.inner.stopped.store(true, Ordering::Relaxed);
        self.inner.stop.notify_one();
        if let Some(thread) = self.inner.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}
async fn run(
    options: BackendOptions,
    state: Arc<Inner>,
    mut requests: channel::UnboundedReceiver<Request>,
    mut manual: channel::UnboundedReceiver<Option<Vec<String>>>,
) -> Result<()> {
    let initialized = async {
        std::fs::create_dir_all(&options.directory)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(options.directory.join("desktop.lock"))?;
        lock.try_lock_exclusive()
            .context("This desktop database is already open in another application instance")?;
        let config = Config::load(&options.directory, options.config.as_deref())?;
        let store = Arc::new(Store::open(&options.directory.join("tracker.sqlite")).await?);
        if let Some(legacy) = &options.legacy {
            store
                .import_legacy(legacy)
                .await
                .context("Legacy import failed; original Node database left unchanged")?;
        }
        store.configure(&config).await?;
        Ok::<_, anyhow::Error>((lock, config, store))
    }
    .await;
    let (lock, config, store) = match initialized {
        Ok(value) => value,
        Err(error) => {
            *state.error.lock().unwrap() = Some(format!("{error:#}"));
            return Err(error);
        }
    };
    let credentials = if options.polling {
        options
            .token
            .or_else(|| options.discover_credentials.then(config::token).flatten())
    } else {
        None
    };
    let polling = credentials.is_some();
    let message = if polling {
        "Local tracker · GitHub polling active"
    } else {
        "Offline · provide a GitHub token, then reopen"
    };
    // A dedicated watcher wakes Rune only after database changes, coalescing write bursts.
    let mut changes = store.db.watch_tables(
        false,
        [
            "items",
            "authors",
            "identities",
            "repositories",
            "maintainers",
            "engagement_events",
            "engagement_sync",
            "advice",
            "poll_state",
            "relationships",
        ],
    );
    let notify = state.clone();
    let watcher = tokio::spawn(async move {
        while changes.next().await.is_some() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            notify.revision.fetch_add(1, Ordering::Relaxed);
            if let Some(wake) = notify.wake.lock().unwrap().as_ref() {
                wake();
            }
        }
    });
    state.revision.fetch_add(1, Ordering::Relaxed);
    if let Some(wake) = state.wake.lock().unwrap().as_ref() {
        wake();
    }
    let github = credentials
        .map(GitHub::new)
        .transpose()?
        .map(SharedGitHub::new);
    if let Some(g)=&github{g.0.lock().await.restore_cooldown(options.directory.join("developer-work/cooldown.json"));}
    let auth_failed=match &github {Some(g)=>g.0.lock().await.auth_failed.clone(),None=>Default::default()};
    let repository_manager = Arc::new(RepositoryManager::new(
        options.directory.join("repositories.json"),
        config.clone(),
    )?);
    let roster_path=options.config.as_ref().and_then(|p|p.parent()).unwrap_or(&options.directory).join(&config.maintainers_file);
    let maintainer_manager=Arc::new(crate::maintainer_config::MaintainerManager::new(roster_path,config.maintainers.clone())?);
    let current=options.directory.join("vips.json");
    let legacy=options.directory.join("customers.json");
    let customer_path=if !current.exists() && legacy.exists(){legacy}else{current};
    if !customer_path.exists(){crate::customer_config::write_file(&customer_path,&[])?;}
    let customers=crate::customer_config::read_valid(&customer_path)
        .or_else(|_|crate::customer_config::read_valid(&customer_path.with_extension("last-good.json"))).unwrap_or_default();
    store.configure_customers(&customers).await?;
    let customer_manager=Arc::new(crate::customer_config::CustomerManager::new(customer_path,customers)?);

    let poll_github = github.clone();
    let poll_state = state.clone();
    let poll_store = store.clone();
    let poller = tokio::spawn(async move {
        let mut poller = poll_github.map(|github| {
            Poller::new(
                poll_store,
                github,
                config.interval,
                poll_state.stopped.clone(),
                poll_state.status.clone(),
            )
        });
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if poll_state.stopped.load(Ordering::Relaxed) {
                break;
            }
            tokio::select! {
                biased;
                _=poll_state.stop.notified()=>break,
                Some(scope)=manual.recv()=>{
                    if let Some(poller)=&mut poller{let _=poller.hard_refresh_repositories(scope).await;}
                    else{*poll_state.status.lock().unwrap()=RefreshStatus{status:"failed".into(),message:"GitHub polling is off. Provide a GitHub token, then reopen the app.".into(),repo:scope.as_ref().filter(|r|r.len()==1).map(|r|r[0].clone()),repositories:scope};}
                    poll_state.revision.fetch_add(1,Ordering::Relaxed);if let Some(wake)=poll_state.wake.lock().unwrap().as_ref(){wake();}
                },
                _=tick.tick()=>{if let Some(poller)=&mut poller{if let Err(_error)=poller.tick().await{/* Individual imports retain their errors and retry state in the database. */}}}
            }
        }
        Ok::<(), anyhow::Error>(())
    });
    // Run the first scheduled import immediately, without blocking reads or the window.
    let mut queries = tokio::task::JoinSet::new();
    let cache_lock = Arc::new(Mutex::new(()));
    let developer_lock=Arc::new(tokio::sync::Mutex::new(()));
    let mut config_watch = tokio::time::interval(Duration::from_secs(1));
    let mut previous_auth_failed=false;
    loop {
        if state.stopped.load(Ordering::Relaxed) {
            break;
        }
        let failed=auth_failed.load(Ordering::Relaxed);
        if failed!=previous_auth_failed {previous_auth_failed=failed;state.revision.fetch_add(1,Ordering::Relaxed);if let Some(wake)=state.wake.lock().unwrap().as_ref(){wake();}}
        tokio::select! {
            _=tokio::time::sleep(Duration::from_millis(100))=>{},
            _=config_watch.tick()=>{if repository_manager.reload(&store).await | maintainer_manager.reload(&store).await | customer_manager.reload(&store).await{state.revision.fetch_add(1,Ordering::Relaxed);if let Some(wake)=state.wake.lock().unwrap().as_ref(){wake();}}},
            Some(_)=queries.join_next(), if !queries.is_empty()=>{},
            Some(request)=requests.recv()=>{
                let store=store.clone();let directory=options.directory.clone();let cache_lock=cache_lock.clone();let repository_manager=repository_manager.clone();let maintainer_manager=maintainer_manager.clone();let customer_manager=customer_manager.clone();let github=github.clone();let auth_failed=auth_failed.clone();let developer_lock=developer_lock.clone();let worker=state.clone();
                queries.spawn(async move{
                    let result=match request.query{
                        Query::Developer(login,closed,force,queue)=>{
                            let result=tokio::time::timeout(Duration::from_secs(840),async {
                                let _guard=developer_lock.lock().await;
                                crate::developer::load_progress(&directory,&store,github,&login,closed,force,&queue,&|message|{
                                    *worker.developer_progress.lock().unwrap()=Some(format!("{login} · {message}"));worker.revision.fetch_add(1,Ordering::Relaxed);if let Some(wake)=worker.wake.lock().unwrap().as_ref(){wake();}
                                }).await
                            }).await;
                            *worker.developer_progress.lock().unwrap()=None;worker.revision.fetch_add(1,Ordering::Relaxed);if let Some(wake)=worker.wake.lock().unwrap().as_ref(){wake();}
                            match result{Ok(result)=>result,Err(_)=>Err(anyhow::anyhow!("Developer refresh timed out. Saved results are unchanged; try refreshing again or reducing enabled repositories."))}
                        },
                        Query::ReplaceToken(token)=>{if let Some(github)=github{let client=match GitHub::new(token){Ok(client)=>client,Err(error)=>{let _=request.reply.send(Err(error.to_string()));return;}};github.replace(client).await;Ok(json!(null))}else{Err(anyhow::anyhow!("GitHub polling is disabled"))}},
                        Query::Snapshot=>store.followup().await.map(|mut data|{data["backend"]=json!({"developer_progress":worker.developer_progress.lock().unwrap().clone(),"auth_failed":auth_failed.load(Ordering::Relaxed),"polling":polling && !auth_failed.load(Ordering::Relaxed),"message":message,"config_error":([repository_manager.current_error(),maintainer_manager.current_error(),customer_manager.current_error()].into_iter().flatten().reduce(|a,b|format!("{a} · {b}")))});if let Ok(raw)=serde_json::to_vec(&data){let _guard=cache_lock.lock().unwrap();let _=std::fs::write(directory.join("snapshot.json.new"),raw);let _=std::fs::rename(directory.join("snapshot.json.new"),directory.join("snapshot.json"));}data}),
                        Query::Maintainers=>{maintainer_manager.reload(&store).await;serde_json::to_value(maintainer_manager.settings().await).map_err(anyhow::Error::from)},
                        Query::Customers=>{customer_manager.reload(&store).await;serde_json::to_value(customer_manager.settings().await).map_err(anyhow::Error::from)},
                        Query::SaveMaintainers(people,revision)=>maintainer_manager.save(&store,people,&revision,github).await.and_then(|settings|Ok(serde_json::to_value(settings)?)),
                        Query::SaveCustomers(people,revision)=>customer_manager.save(&store,people,&revision,github).await.and_then(|settings|Ok(serde_json::to_value(settings)?)),
                        Query::Repositories=>{repository_manager.reload(&store).await;serde_json::to_value(repository_manager.settings().await).map_err(anyhow::Error::from)},
                        Query::SaveRepositories(repos,revision)=>repository_manager.save(&store,repos,&revision,github).await.and_then(|settings|Ok(serde_json::to_value(settings)?)),
                        Query::Detail(key)=>store.detail(&key).await,
                        Query::Discussion(key,page,force)=>match store.detail(&key).await {
                            Ok(item)=>crate::discussion::load(&directory,&item,page,force,github).await,
                            Err(error)=>Err(error),
                        },
                        Query::Advice(key,text,enabled)=>store.advice(&key,&text,enabled).await,
                    };let _=request.reply.send(result.map_err(|e|format!("{e:#}")));
                });
            }
        }
    }
    // Dropping an in-flight HTTP future cancels it. SQL transactions contain no await points.
    poller.abort();
    let _ = poller.await;
    watcher.abort();
    let _ = watcher.await;
    queries.shutdown().await;
    store.db.disconnect().await;
    // Reader/writer leases and the process lock are released when the worker runtime drops.
    drop(lock);
    Ok(())
}

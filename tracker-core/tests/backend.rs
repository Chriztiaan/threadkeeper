use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicBool},
};
use tracker_core::{
    config::{Config, Maintainer},
    github::{Page, Source},
    poller::{Poller, RefreshStatus},
    store::{Store, engagement_pen, references},
};
fn config() -> Config {
    let mut c:Config=serde_json::from_value(json!({"repositories":[{"name":"acme/sdk","group":"SDK"},{"name":"acme/core","group":"Core"}]})).unwrap();
    c.maintainers = vec![Maintainer {
        github_id: "8".into(),
        login: "maintainer".into(),
    }];
    c
}
fn user(key: u64, kind: &str) -> Value {
    json!({"id":key,"login":format!("user{key}"),"type":kind,"avatar_url":"","html_url":""})
}
fn item(key: u64) -> Value {
    json!({"id":key,"node_id":format!("I_{key}"),"number":key,"title":format!("Issue {key}"),"body":"See #2 and acme/core#3. `#5`\n\n```\n#6\n```","state":"open","user":user(7,"User"),"labels":[{"name":"bug","color":"ff0000"}],"assignees":[],"comments":0,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-11-01T00:00:00Z","closed_at":null,"html_url":format!("https://github.com/acme/sdk/issues/{key}")})
}
async fn init(path: &std::path::Path) -> Store {
    let store = Store::open(path).await.unwrap();
    store.configure(&config()).await.unwrap();
    store
        .save_repository(
            "acme/sdk",
            &json!({"id":10,"full_name":"acme/sdk","html_url":""}),
        )
        .await
        .unwrap();
    store
        .save_repository(
            "acme/core",
            &json!({"id":20,"full_name":"acme/core","html_url":""}),
        )
        .await
        .unwrap();
    store
}
#[tokio::test]
async fn store_keeps_external_unknown_and_draft_prs_excludes_maintainers_bots_and_closed_and_saves_local_advice()
 {
    let dir = tempfile::tempdir().unwrap();
    let store = init(&dir.path().join("db.sqlite")).await;
    let repo = store.repository("acme/sdk").await.unwrap();
    let mut maintainer = item(2);
    maintainer["user"] = user(8, "User");
    let mut bot = item(3);
    bot["user"] = user(9, "Bot");
    let mut unknown = item(4);
    unknown["user"] = Value::Null;
    let mut closed = item(5);
    closed["state"] = json!("closed");
    let mut pr = item(6);
    pr["pull_request"] = json!({});
    pr["draft"] = json!(true);
    store
        .save_items(
            &repo,
            &[item(1), maintainer, bot, unknown, closed, pr],
            &HashMap::new(),
        )
        .await
        .unwrap();
    let pending = store.followup().await.unwrap();
    assert_eq!(pending["total"], 3);
    assert_eq!(pending["pending"]["total"], 3);
    assert_eq!(pending["pens"][4]["total"], 0);
    let events = vec![
        json!({"id":"human","kind":"review","at":"2026-11-09T12:00:00Z","user":user(8,"User"),"url":""}),
        json!({"id":"bot","kind":"comment","at":"2026-11-10T12:00:00Z","user":user(9,"Bot"),"url":""}),
    ];
    store
        .save_engagement("1", &events, "2026-11-01T00:00:00Z")
        .await
        .unwrap();
    store.advice("1", "Keep this advice", true).await.unwrap();
    store
        .save_items(&repo, &[item(1)], &HashMap::new())
        .await
        .unwrap();
    let detail = store.detail("1").await.unwrap();
    assert_eq!(detail["engagement"]["kind"], "review");
    assert_eq!(detail["engagement"]["author"]["maintainer"], true);
    assert_eq!(detail["advice"]["text"], "Keep this advice");
    assert_eq!(detail["labels"][0]["name"], "bug");
    assert_eq!(store.detail("6").await.unwrap()["state"], "draft");
    assert_eq!(
        store
            .one("SELECT count(*) AS n FROM ps_crud", &[])
            .await
            .unwrap()["n"],
        0
    );
    assert!(store.advice("1", &"a".repeat(20001), true).await.is_err());
}
#[tokio::test]
async fn migration_preserves_ids_advice_and_checkpoints_once_without_touching_source() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy.sqlite");
    let source = init(&legacy).await;
    let repo = source.repository("acme/sdk").await.unwrap();
    source
        .save_items(&repo, &[item(1)], &HashMap::new())
        .await
        .unwrap();
    source.advice("1", "Original note", false).await.unwrap();
    source
        .update_poll(
            "10",
            json!({"stage":"ready","checkpoint":"2026-11-09T00:00:00Z"}),
        )
        .await
        .unwrap();
    let destination = Store::open(&dir.path().join("native.sqlite"))
        .await
        .unwrap();
    assert_eq!(destination.import_legacy(&legacy).await.unwrap(), 1);
    assert_eq!(
        destination.poll_state("10").await.unwrap()["checkpoint"],
        "2026-11-09T00:00:00Z"
    );
    assert_eq!(
        destination.detail("1").await.unwrap()["advice"]["enabled"],
        false
    );
    destination.advice("1", "Native note", true).await.unwrap();
    assert_eq!(destination.import_legacy(&legacy).await.unwrap(), 0);
    assert_eq!(
        destination.detail("1").await.unwrap()["advice"]["text"],
        "Native note"
    );
    assert_eq!(
        source.detail("1").await.unwrap()["advice"]["text"],
        "Original note"
    );
    drop(destination);
    let reopened = Store::open(&dir.path().join("native.sqlite"))
        .await
        .unwrap();
    assert_eq!(
        reopened.detail("1").await.unwrap()["advice"]["text"],
        "Native note"
    );
}
#[test]
fn pens_use_exact_boundaries_and_references_skip_markdown_code() {
    let now: DateTime<Utc> = "2026-10-30T12:00:00Z".parse().unwrap();
    for (at, pen) in [
        ("2026-10-29T12:00:01Z", "day"),
        ("2026-10-29T12:00:00Z", "week"),
        ("2026-10-23T12:00:00Z", "month"),
        ("2026-09-30T12:00:00Z", "six-months"),
        ("2026-04-30T12:00:00Z", "six-months"),
        ("2026-04-30T11:59:59Z", "cold"),
    ] {
        assert_eq!(engagement_pen(at, now).unwrap(), pen);
    }
    let refs = references(
        "See #2, acme/core#3 and https://github.com/acme/sdk/pull/4. `#5`\n\n```\n#6\n```",
        "acme/sdk",
    );
    assert_eq!(refs.len(), 3);
}
struct Mock {
    fail: bool,
    calls: Arc<Mutex<Vec<String>>>,
}
#[async_trait::async_trait]
impl Source for Mock {
    async fn repository(&mut self, name: &str) -> anyhow::Result<Value> {
        Ok(json!({"id":if name=="acme/sdk"{10}else{20},"full_name":name,"html_url":""}))
    }
    async fn issues(
        &mut self,
        name: &str,
        _state: &str,
        page: u64,
        since: Option<&str>,
        _sort: &str,
    ) -> anyhow::Result<Page> {
        self.calls.lock().unwrap().push(format!("{name}:{page}"));
        assert!(since.is_some());
        if page == 2 && self.fail {
            anyhow::bail!("Disconnected");
        }
        let mut value = item(page);
        if page == 1 {
            value["state"] = json!("closed");
        } else {
            value["pull_request"] = json!({});
        }
        Ok(Page {
            items: vec![value],
            next: if page == 1 { Some(2) } else { None },
        })
    }
    async fn pull(&mut self, _name: &str, _number: u64) -> anyhow::Result<Value> {
        Ok(json!({"isDraft":false,"mergedAt":"2026-11-09T00:00:00Z","closing":[]}))
    }
    async fn engagement(
        &mut self,
        name: &str,
        number: u64,
        _kind: &str,
    ) -> anyhow::Result<Vec<Value>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("discussion:{name}:{number}"));
        Ok(vec![])
    }
}
#[tokio::test]
async fn failed_page_retains_checkpoint_and_retry_catches_closures_and_merged_prs() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(init(&dir.path().join("db.sqlite")).await);
    let repo = store.repository("acme/sdk").await.unwrap();
    store
        .save_items(&repo, &[item(1)], &HashMap::new())
        .await
        .unwrap();
    store.update_poll("10",json!({"stage":"ready","checkpoint":"2026-10-19T12:00:00Z","last_success":"2026-10-19T12:00:00Z"})).await.unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let status = Arc::new(Mutex::new(RefreshStatus::default()));
    let stopped = Arc::new(AtomicBool::new(false));
    let mut poller = Poller::new(
        store.clone(),
        Mock {
            fail: true,
            calls: calls.clone(),
        },
        300,
        stopped.clone(),
        status.clone(),
    );
    poller.step("acme/sdk", true).await.unwrap();
    assert_eq!(
        store.poll_state("10").await.unwrap()["checkpoint"],
        "2026-10-19T12:00:00Z"
    );
    assert!(!store.poll_state("10").await.unwrap()["error"].is_null());
    store
        .update_poll("10", json!({"next_retry":null}))
        .await
        .unwrap();
    let mut poller = Poller::new(
        store.clone(),
        Mock {
            fail: false,
            calls: calls.clone(),
        },
        300,
        stopped,
        status,
    );
    poller.hard_refresh(Some("acme/sdk".into())).await.unwrap();
    assert_eq!(store.detail("1").await.unwrap()["state"], "closed");
    assert_eq!(store.detail("2").await.unwrap()["state"], "merged");
    assert_ne!(
        store.poll_state("10").await.unwrap()["checkpoint"],
        "2026-10-19T12:00:00Z"
    );
    assert!(
        !calls
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.contains("acme/core"))
    );
}
#[test]
fn embedded_worker_runs_without_node_and_releases_database_lock_on_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let options = tracker_core::BackendOptions {
        directory: dir.path().into(),
        legacy: None,
        config: None,
        token: None,
        discover_credentials: false,
        polling: false,
    };
    let backend = tracker_core::Backend::start(options.clone()).unwrap();
    assert!(backend.snapshot().unwrap()["total"].is_number());
    let second = tracker_core::Backend::start(options.clone()).unwrap();
    assert!(
        second
            .snapshot()
            .unwrap_err()
            .to_string()
            .contains("already open")
    );
    second.shutdown();
    backend.shutdown();
    let reopened = tracker_core::Backend::start(options).unwrap();
    assert_eq!(reopened.snapshot().unwrap()["total"], 0);
    reopened.shutdown();
}

#[tokio::test]
async fn embedded_database_changes_notify_the_ui_without_http_polling() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("source.sqlite");
    let source = init(&legacy).await;
    let repo = source.repository("acme/sdk").await.unwrap();
    source
        .save_items(&repo, &[item(1)], &HashMap::new())
        .await
        .unwrap();
    std::fs::create_dir_all(dir.path().join("desktop")).unwrap();
    std::fs::write(
        dir.path().join("desktop/tracker.config.json"),
        serde_json::to_vec(&config()).unwrap(),
    )
    .unwrap();
    let backend = tracker_core::Backend::start(tracker_core::BackendOptions {
        directory: dir.path().join("desktop"),
        legacy: Some(legacy),
        config: None,
        token: None,
        discover_credentials: false,
        polling: false,
    })
    .unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let signal = wakes.clone();
    backend.set_waker(move || {
        signal.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(backend.snapshot().unwrap()["total"], 1);
    let before = backend.revision();
    let wakes_before = wakes.load(Ordering::Relaxed);
    backend
        .advice("1".into(), "Changed locally".into(), true)
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    assert!(backend.revision() > before);
    assert!(wakes.load(Ordering::Relaxed) > wakes_before);
    assert_eq!(
        backend.detail("1".into()).unwrap()["advice"]["text"],
        "Changed locally"
    );
    backend.shutdown();
}

struct RefreshRecorder(Arc<Mutex<Vec<String>>>);
#[async_trait::async_trait]
impl Source for RefreshRecorder {
    async fn repository(&mut self, name: &str) -> anyhow::Result<Value> {
        Ok(
            json!({"id":if name=="acme/sdk"{10}else if name=="acme/core"{20}else{30},"full_name":name,"html_url":""}),
        )
    }
    async fn issues(
        &mut self,
        name: &str,
        _state: &str,
        _page: u64,
        _since: Option<&str>,
        _sort: &str,
    ) -> anyhow::Result<Page> {
        self.0.lock().unwrap().push(name.into());
        Ok(Page {
            items: vec![],
            next: None,
        })
    }
    async fn pull(&mut self, _name: &str, _number: u64) -> anyhow::Result<Value> {
        unreachable!()
    }
    async fn engagement(
        &mut self,
        _name: &str,
        _number: u64,
        _kind: &str,
    ) -> anyhow::Result<Vec<Value>> {
        Ok(vec![])
    }
}
#[tokio::test]
async fn multi_repository_refresh_only_visits_selected_repositories_and_empty_is_not_all() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(init(&dir.path().join("db.sqlite")).await);
    let mut cfg = config();
    cfg.repositories.push(tracker_core::config::Repository {
        name: "acme/ignored".into(),
        include_terms: vec![],
        enabled: true,
    });
    store.configure(&cfg).await.unwrap();
    for (id, name) in [(10, "acme/sdk"), (20, "acme/core"), (30, "acme/ignored")] {
        store
            .save_repository(name, &json!({"id":id,"full_name":name,"html_url":""}))
            .await
            .unwrap();
        store
            .update_poll(
                &id.to_string(),
                json!({"stage":"ready","checkpoint":"2026-11-01T00:00:00Z"}),
            )
            .await
            .unwrap();
    }
    let calls = Arc::new(Mutex::new(vec![]));
    let mut poller = Poller::new(
        store,
        RefreshRecorder(calls.clone()),
        300,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(RefreshStatus::default())),
    );
    poller
        .hard_refresh_repositories(Some(vec!["acme/sdk".into(), "acme/core".into()]))
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), vec!["acme/sdk", "acme/core"]);
    assert!(
        poller
            .hard_refresh_repositories(Some(vec![]))
            .await
            .is_err()
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn repository_management_removes_without_erasing_and_file_reload_restores_saved_advice() {
    use tracker_core::repository_config::{RepositoryManager, write_file};
    let dir = tempfile::tempdir().unwrap();
    let store = init(&dir.path().join("tracker.sqlite")).await;
    let repo = store.repository("acme/sdk").await.unwrap();
    store
        .save_items(&repo, &[item(1)], &HashMap::new())
        .await
        .unwrap();
    store.advice("1", "Keep this note", true).await.unwrap();
    store
        .update_poll("10", json!({"checkpoint":"2026-11-09T00:00:00Z"}))
        .await
        .unwrap();
    let path = dir.path().join("repositories.json");
    let cfg = config();
    write_file(&path, &cfg.repositories).unwrap();
    let manager = RepositoryManager::new(path.clone(), cfg.clone()).unwrap();
    let before = manager.settings().await;
    manager
        .save(
            &store,
            vec![cfg.repositories[1].clone()],
            &before.revision,
            None,
        )
        .await
        .unwrap();
    assert_eq!(store.followup().await.unwrap()["total"], 0);
    assert_eq!(
        store
            .one("SELECT count(*) AS n FROM items", &[])
            .await
            .unwrap()["n"],
        1
    );
    assert!(store.detail("1").await.is_err());
    let state = manager.settings().await;
    manager
        .save(&store, vec![], &state.revision, None)
        .await
        .unwrap();
    assert!(store.repositories().await.unwrap().is_empty());
    write_file(&path, &cfg.repositories).unwrap();
    assert!(manager.reload(&store).await);
    assert_eq!(
        store.detail("1").await.unwrap()["advice"]["text"],
        "Keep this note"
    );
    assert_eq!(
        store.poll_state("10").await.unwrap()["checkpoint"],
        "2026-11-09T00:00:00Z"
    );
}
#[tokio::test]
async fn repository_file_conflicts_and_invalid_edits_keep_last_working_configuration() {
    use tracker_core::repository_config::{RepositoryManager, write_file};
    let dir = tempfile::tempdir().unwrap();
    let store = init(&dir.path().join("tracker.sqlite")).await;
    let path = dir.path().join("repositories.json");
    let cfg = config();
    write_file(&path, &cfg.repositories).unwrap();
    let manager = RepositoryManager::new(path.clone(), cfg.clone()).unwrap();
    let original = manager.settings().await;
    let mut external = cfg.repositories.clone();
    external[0].include_terms = vec!["changed in editor".into()];
    write_file(&path, &external).unwrap();
    assert!(
        manager
            .save(&store, cfg.repositories.clone(), &original.revision, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("file changed")
    );
    assert!(manager.reload(&store).await);
    assert_eq!(
        manager.settings().await.repositories[0].include_terms,
        vec!["changed in editor"]
    );
    std::fs::write(&path, "{ invalid json").unwrap();
    assert!(manager.reload(&store).await);
    assert!(manager.settings().await.error.is_some());
    assert_eq!(store.repositories().await.unwrap().len(), 2);
    // A restart can recover the last known good file without overwriting the broken edit.
    std::fs::write(
        dir.path().join("tracker.config.json"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .unwrap();
    let recovered = Config::load(dir.path(), None).unwrap();
    assert_eq!(recovered.repositories[0].include_terms, vec!["changed in editor"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ invalid json");
}
#[test]
fn repository_names_accept_urls_and_reject_duplicates_and_invalid_hosts() {
    use tracker_core::repository_config::{normalize_name, validated};
    assert_eq!(
        normalize_name(" https://github.com/Acme/SDK.git/ ").unwrap(),
        "acme/sdk"
    );
    assert!(normalize_name("https://example.com/acme/sdk").is_err());
    assert!(normalize_name("https://github.com/acme/sdk/issues").is_err());
    let mut repos = config().repositories;
    repos[1].name = "https://github.com/ACME/sdk".into();
    assert!(validated(repos, &config()).is_err());
    assert!(validated(vec![], &config()).is_ok());
}
#[test]
fn embedded_repository_preferences_apply_ui_saves_and_external_file_edits_without_restart() {
    use tracker_core::{Backend, BackendOptions, repository_config::write_file};
    let dir = tempfile::tempdir().unwrap();
    let backend = Backend::start(BackendOptions {
        directory: dir.path().into(),
        legacy: None,
        config: None,
        token: None,
        discover_credentials: false,
        polling: false,
    })
    .unwrap();
    let settings = backend.repository_settings().unwrap();
    let result = backend
        .save_repositories(vec![], settings.revision)
        .unwrap();
    assert!(result.repositories.is_empty());
    assert_eq!(backend.snapshot().unwrap()["repositories"], json!([]));
    let repo = tracker_core::config::Repository {
        name: "acme/new".into(),
        include_terms: vec![],
        enabled: true,
    };
    let settings = backend.repository_settings().unwrap();
    assert!(
        backend
            .save_repositories(vec![repo.clone()], settings.revision)
            .is_err()
    );
    write_file(&dir.path().join("repositories.json"), &[repo]).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while backend.snapshot().unwrap()["repositories"] != json!(["acme/new"])
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(
        backend.snapshot().unwrap()["repositories"],
        json!(["acme/new"])
    );
    std::fs::write(dir.path().join("repositories.json"), "not json").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while backend.snapshot().unwrap()["backend"]["config_error"].is_null()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(backend.snapshot().unwrap()["backend"]["config_error"].is_string());
    assert_eq!(
        backend.snapshot().unwrap()["repositories"],
        json!(["acme/new"])
    );
    backend.shutdown();
}

#[tokio::test]
async fn maintainer_files_reclassify_entries_preserve_data_and_do_not_revert_repository_edits(){
    use tracker_core::{maintainer_config::{MaintainerManager,write_file},repository_config::RepositoryManager};
    let dir=tempfile::tempdir().unwrap();let store=init(&dir.path().join("db.sqlite")).await;
    let repo=store.repository("acme/sdk").await.unwrap();store.save_items(&repo,&[item(1)],&HashMap::new()).await.unwrap();
    store.advice("1","Keep this note",true).await.unwrap();
    let mut cfg=config();cfg.maintainers=vec![Maintainer{github_id:"7".into(),login:"user7".into()}];
    store.configure_maintainers(&cfg.maintainers).await.unwrap();
    let path=dir.path().join("maintainers.json");write_file(&path,&cfg.maintainers).unwrap();
    let manager=MaintainerManager::new(path.clone(),cfg.maintainers.clone()).unwrap();
    let repo_path=dir.path().join("repositories.json");tracker_core::repository_config::write_file(&repo_path,&cfg.repositories).unwrap();
    let repositories=RepositoryManager::new(repo_path,cfg.clone()).unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],0);
    let settings=manager.settings().await;manager.save(&store,vec![],&settings.revision,None).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],1);
    let settings=repositories.settings().await;repositories.save(&store,cfg.repositories.clone(),&settings.revision,None).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],1,"Saving repositories must not restore the stale roster");
    write_file(&path,&cfg.maintainers).unwrap();assert!(manager.reload(&store).await);
    assert_eq!(store.followup().await.unwrap()["total"],0);
    let stale=manager.settings().await;std::fs::write(&path,"not json").unwrap();assert!(manager.reload(&store).await);
    assert!(manager.settings().await.error.is_some());assert_eq!(store.followup().await.unwrap()["total"],0);
    assert!(manager.save(&store,vec![],&stale.revision,None).await.is_err());
    let settings=manager.settings().await;manager.save(&store,vec![],&settings.revision,None).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],1);
    assert_eq!(store.detail("1").await.unwrap()["advice"]["text"],"Keep this note");
    assert_eq!(tracker_core::maintainer_config::read_valid(&path).unwrap().len(),0);
}

#[test]
fn invalid_maintainer_files_recover_from_last_good_on_restart_and_imports_reject_duplicates(){
    use tracker_core::maintainer_config::{validated,write_file};
    let dir=tempfile::tempdir().unwrap();Config::load(dir.path(),None).unwrap();
    let roster=vec![Maintainer{github_id:"7".into(),login:"User7".into()}];
    write_file(&dir.path().join("maintainers.last-good.json"),&roster).unwrap();
    std::fs::write(dir.path().join("maintainers.json"),"bad json").unwrap();
    assert_eq!(Config::load(dir.path(),None).unwrap().maintainers[0].github_id,"7");
    assert!(validated(vec![roster[0].clone(),Maintainer{github_id:"8".into(),login:"user7".into()}],false).is_err());
    assert!(validated(vec![Maintainer{github_id:String::new(),login:"@alice".into()}],false).is_ok());
    assert!(validated(vec![Maintainer{github_id:String::new(),login:"alice".into()}],true).is_err());
}

#[test]
fn embedded_maintainer_settings_apply_saves_and_external_files_without_restarting(){
    let dir=tempfile::tempdir().unwrap();
    let backend=tracker_core::Backend::start(tracker_core::BackendOptions{directory:dir.path().into(),legacy:None,config:None,token:None,discover_credentials:false,polling:false}).unwrap();
    let initial=backend.maintainer_settings().unwrap();assert!(initial.maintainers.is_empty());
    assert_eq!(backend.snapshot().unwrap()["repositories"],json!([]));
    backend.save_maintainers(vec![],initial.revision).unwrap();assert_eq!(backend.snapshot().unwrap()["maintainerCount"],0);
    let path=dir.path().join("maintainers.json");
    tracker_core::maintainer_config::write_file(&path,&[Maintainer{github_id:"7".into(),login:"alice".into()}]).unwrap();
    let current=backend.maintainer_settings().unwrap();assert_eq!(current.maintainers[0].github_id,"7");assert_eq!(backend.snapshot().unwrap()["maintainerCount"],1);
    std::fs::write(&path,"not json").unwrap();assert!(backend.maintainer_settings().unwrap().error.is_some());
    assert_eq!(backend.snapshot().unwrap()["maintainerCount"],1);backend.shutdown();
}

#[tokio::test]
async fn customers_match_stable_authors_reload_and_preserve_notes() {
    use tracker_core::customer_config::{Customer,CustomerManager,Highlight,write_file};
    let dir=tempfile::tempdir().unwrap();let store=init(&dir.path().join("db.sqlite")).await;
    let repo=store.repository("acme/sdk").await.unwrap();
    let mut pr=item(2);pr["pull_request"]=json!({"url":"https://api.github.com/repos/acme/sdk/pulls/2"});
    store.save_items(&repo,&[item(1),pr],&HashMap::new()).await.unwrap();
    store.advice("1","Keep customer context",true).await.unwrap();
    let customer=Customer{github_id:"7".into(),login:"old-login".into(),full_name:"Alice Example".into(),notes:"Example Org".into(),company:"Acme".into(),highlight:Highlight::Neon};
    let path=dir.path().join("customers.json");write_file(&path,&[customer.clone()]).unwrap();
    store.configure_customers(&[customer.clone()]).await.unwrap();
    let manager=CustomerManager::new(path.clone(),vec![customer.clone()]).unwrap();
    for id in ["1","2"] {let detail=store.detail(id).await.unwrap();assert_eq!(detail["author"]["vip"],true);assert_eq!(detail["author"]["highlight"],"neon");assert_eq!(detail["author"]["notes"],"Example Org");assert_eq!(detail["author"]["company"],"Acme");}
    assert_eq!(store.followup().await.unwrap()["total"],2);
    let mut edited=customer;edited.highlight=Highlight::Outline;edited.notes="New Org".into();
    let settings=manager.settings().await;manager.save(&store,vec![edited],&settings.revision,None).await.unwrap();
    assert_eq!(store.detail("1").await.unwrap()["author"]["highlight"],"outline");
    let stale=manager.settings().await;std::fs::write(&path,"bad JSON").unwrap();assert!(manager.reload(&store).await);
    assert!(manager.settings().await.error.is_some());assert_eq!(store.detail("1").await.unwrap()["author"]["vip"],true);
    assert!(manager.save(&store,vec![],&stale.revision,None).await.is_err());
    assert_eq!(tracker_core::customer_config::read_valid(&path.with_extension("last-good.json")).unwrap()[0].notes,"New Org");
    write_file(&path,&[]).unwrap();assert!(manager.reload(&store).await);
    assert_eq!(store.detail("1").await.unwrap()["author"]["vip"],false);
    assert_eq!(store.detail("1").await.unwrap()["advice"]["text"],"Keep customer context");
}

#[test]
fn customer_import_validates_profiles_ids_metadata_and_style() {
    use tracker_core::customer_config::{CustomerFile,validated,Highlight};
    let parse=|raw:&str|->anyhow::Result<_>{let f:CustomerFile=serde_json::from_str(raw)?;validated(f.customers,false)};
    let rows=parse(r#"{"customers":[{"login":"https://github.com/Alice/","fullName":"Alice Example","organization":"Acme"}]}"#).unwrap();
    assert_eq!(rows[0].login,"Alice");assert_eq!(rows[0].company,"Acme");assert_eq!(rows[0].highlight,Highlight::GoldStar);
    assert!(validated(rows,true).is_err());
    for raw in [r#"{"customers":[{"login":"alice"},{"login":"@ALICE"}]}"#,r#"{"customers":[{"login":"alice","githubId":"7"},{"login":"bob","githubId":"7"}]}"#,r#"{"customers":[{"login":"https://evil.test/alice"}]}"#,r#"{"customers":[{"login":"alice","highlight":"invalid"}]}"#] {assert!(parse(raw).is_err());}
}

#[test]
fn embedded_customer_roster_recovers_after_restart() {
    use tracker_core::customer_config::{Customer,write_file};
    let dir=tempfile::tempdir().unwrap();
    let start=||tracker_core::Backend::start(tracker_core::BackendOptions{directory:dir.path().into(),legacy:None,config:None,token:None,discover_credentials:false,polling:false}).unwrap();
    let backend=start();assert!(backend.customer_settings().unwrap().customers.is_empty());
    let path=dir.path().join("vips.json");write_file(&path,&[Customer{github_id:"7".into(),login:"alice".into(),..Default::default()}]).unwrap();
    let current=backend.customer_settings().unwrap();assert_eq!(current.customers.len(),1);
    std::fs::write(&path,"bad JSON").unwrap();assert!(backend.customer_settings().unwrap().error.is_some());backend.shutdown();drop(backend);
    let backend=start();let current=backend.customer_settings().unwrap();assert_eq!(current.customers.len(),1);assert!(current.error.is_some());
    backend.save_customers(vec![],current.revision).unwrap();assert!(backend.customer_settings().unwrap().customers.is_empty());backend.shutdown();
}

#[test]
fn repository_terms_are_literal_any_case_insensitive_and_groups_are_ignored() {
    use tracker_core::config::{normalize_terms,includes,Repository};
    let terms=normalize_terms(&[" PowerSync ".into(),"POWERSYNC".into(),"".into(),"c++".into(),"CAFÉ".into()]);
    assert_eq!(terms,vec!["powersync","c++","café"]);
    assert!(includes(&terms,"PowerSync integration"));assert!(includes(&terms,"C++ example"));assert!(includes(&terms,"Café"));
    assert!(!includes(&terms,"Power"));assert!(!includes(&terms,"Other"));assert!(includes(&[],""));
    let repo:Repository=serde_json::from_value(json!({"name":"acme/sdk","group":"Legacy"})).unwrap();
    assert!(repo.include_terms.is_empty());assert!(serde_json::to_value(repo).unwrap().get("group").is_none());
}

#[tokio::test]
async fn repository_filters_recheck_cached_entries_and_preserve_updates_and_advice() {
    let dir=tempfile::tempdir().unwrap();let store=init(&dir.path().join("db.sqlite")).await;
    let mut cfg=config();cfg.repositories[0].include_terms=vec!["powersync".into(),"journeyapps".into()];store.configure_repositories(&cfg).await.unwrap();
    let stale_repo=store.repository("acme/sdk").await.unwrap();
    let mut a=item(1);a["title"]=json!("PowerSync integration");
    let mut b=item(2);b["title"]=json!("Works with JourneyApps");b["pull_request"]=json!({});
    let mut c=item(3);c["body"]=json!("PowerSync in description only");
    store.save_items(&stale_repo,&[a.clone(),b.clone(),c.clone()],&HashMap::new()).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],2);assert_eq!(store.candidates(100,0,None).await.unwrap().len(),2);
    assert!(!store.should_enrich("acme/sdk",&c).await.unwrap());
    store.advice("1","Keep this",true).await.unwrap();
    cfg.repositories[0].include_terms=vec!["unrelated".into()];store.configure_repositories(&cfg).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],0);
    // A page fetched with an older config must use the latest filter when committed.
    store.save_items(&stale_repo,&[a.clone()],&HashMap::new()).await.unwrap();assert_eq!(store.followup().await.unwrap()["total"],0);
    b["title"]=json!("Renamed");b["body"]=json!("");b["state"]=json!("closed");b["updated_at"]=json!("2026-11-10T00:00:00Z");
    assert!(store.should_enrich("acme/sdk",&b).await.unwrap());
    store.save_items(&stale_repo,&[b.clone()],&HashMap::from([(2,json!({"mergedAt":"2026-11-10T00:00:00Z","closing":[]}))])).await.unwrap();
    assert_eq!(store.detail("2").await.unwrap()["state"],"merged");
    cfg.repositories[0].include_terms.clear();store.configure_repositories(&cfg).await.unwrap();assert_eq!(store.followup().await.unwrap()["total"],2);
    assert_eq!(store.detail("1").await.unwrap()["advice"]["text"],"Keep this");
    // An older listing does not overwrite newer text, classification or merge status.
    store.save_items(&stale_repo,&[item(2)],&HashMap::new()).await.unwrap();assert_eq!(store.detail("2").await.unwrap()["state"],"merged");
}

#[tokio::test]
async fn title_filter_migration_rechecks_previously_included_description_matches() {
    let dir=tempfile::tempdir().unwrap();let store=init(&dir.path().join("db.sqlite")).await;
    let mut cfg=config();cfg.repositories[0].include_terms=vec!["powersync".into()];store.configure_repositories(&cfg).await.unwrap();
    let repo=store.repository("acme/sdk").await.unwrap();let mut entry=item(1);entry["body"]=json!("PowerSync");
    store.save_items(&repo,&[entry],&HashMap::new()).await.unwrap();store.advice("1","Preserve note",true).await.unwrap();
    store.execute("UPDATE items SET included=1,ever_included=1",&[]).await.unwrap();
    store.execute("DELETE FROM tracker_meta WHERE key='repository_filter_version'",&[]).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],1);
    store.configure_repositories(&cfg).await.unwrap();
    assert_eq!(store.followup().await.unwrap()["total"],0);assert!(store.candidates(100,0,None).await.unwrap().is_empty());
    assert_eq!(store.detail("1").await.unwrap()["advice"]["text"],"Preserve note");
}

#[tokio::test]
async fn repository_sync_exposes_queue_import_poll_and_failure_states_without_matches() {
    let dir=tempfile::tempdir().unwrap();let store=init(&dir.path().join("db.sqlite")).await;
    let mut cfg=config();cfg.repositories.push(serde_json::from_value(json!({"name":"external/new"})).unwrap());store.configure_repositories(&cfg).await.unwrap();
    let snapshot=store.followup().await.unwrap();let sync=snapshot["repositorySync"].as_array().unwrap();
    assert_eq!(sync.iter().find(|r|r["repo"]=="external/new").unwrap()["status"],"queued");
    for (changes,expected) in [
        (json!({"stage":"opening","active":1}),"importing"),
        (json!({"stage":"backfill","backfill_page":3}),"importing"),
        (json!({"active":0}),"waiting"),
        (json!({"stage":"ready","active":1}),"polling"),
        (json!({"active":0,"last_success":"2026-11-11T12:00:00Z"}),"ready"),
        (json!({"error":"Rate limit reached","next_retry":"2026-11-11T13:00:00Z"}),"error"),
    ] {
        store.update_poll("10",changes).await.unwrap();let snapshot=store.followup().await.unwrap();
        let repo=snapshot["repositorySync"].as_array().unwrap().iter().find(|r|r["repo"]=="acme/sdk").unwrap();
        assert_eq!(repo["status"],expected);assert_eq!(repo["matched"],0);
    }
    store.configure(&cfg).await.unwrap();assert_eq!(store.poll_state("10").await.unwrap()["active"],0);
}

#[test]
fn vip_rosters_preserve_legacy_company_and_notes_and_write_canonical_format() {
    use tracker_core::customer_config::{CustomerFile,read_valid,write_file};
    let legacy:CustomerFile=serde_json::from_str(r#"{"customers":[{"githubId":"7","login":"alice","company":"Acme","organization":"Partner team","notes":"Follow up Friday\nPrefers examples"}]}"#).unwrap();
    assert_eq!(legacy.customers[0].notes,"Follow up Friday\nPrefers examples");
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("vips.json");
    write_file(&path,&legacy.customers).unwrap();
    let raw:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(raw.get("customers").is_none());assert_eq!(raw["vips"][0]["company"],"Acme / Partner team");
    assert_eq!(read_valid(&path).unwrap()[0].notes,legacy.customers[0].notes);
}

#[test]
fn discussion_text_handles_bot_html_without_losing_code_or_link_targets() {
    use tracker_core::discussion::display_body;
    let body="**Actionable comments posted: 1**\n\n<details><summary><b>Review notes</b></summary>\n\n- [Example](https://example.com/repro)\n\n</details>";
    let text=display_body(body);
    assert!(text.contains("Actionable comments posted: 1"));assert!(text.contains("Review notes"));assert!(text.contains("https://example.com/repro"));assert!(!text.contains("<details>"));assert!(!text.contains("**"));
    assert!(display_body("```rust\nVec<T>\n```").contains("Vec<T>"));
    assert!(display_body("<!-- hidden -->\n\n").is_empty());
}

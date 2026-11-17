use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};
use tracker_core::github::{GitHub, Source};
fn server(
    responses: Vec<String>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(Mutex::new(vec![]));
    let recorded = calls.clone();
    let thread = std::thread::spawn(move || {
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..n]);
                if let Some(end) = data.windows(4).position(|s| s == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]).to_lowercase();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            recorded
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&data).into());
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    (base, calls, thread)
}
fn response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
        body.len()
    )
}
#[tokio::test]
async fn etags_revalidate_data_and_preserve_next_page_links() {
    let (base, calls, thread) = server(vec![
        response(
            "200 OK",
            "ETag: \"a\"\r\nLink: <http://example.test?page=2>; rel=\"next\"\r\n",
            "[]",
        ),
        response("304 Not Modified", "", ""),
    ]);
    let mut client = GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap();
    let first = client
        .issues("acme/sdk", "all", 1, None, "updated")
        .await
        .unwrap();
    let second = client
        .issues("acme/sdk", "all", 1, None, "updated")
        .await
        .unwrap();
    assert_eq!(first.next, Some(2));
    assert_eq!(second.next, Some(2));
    thread.join().unwrap();
    assert!(
        calls.lock().unwrap()[1]
            .to_lowercase()
            .contains("if-none-match: \"a\"")
    );
}
#[tokio::test]
async fn rate_limits_and_authentication_failures_block_further_network_requests() {
    for (status, headers) in [
        ("429 Too Many Requests", "Retry-After: 120\r\n"),
        ("401 Unauthorized", ""),
    ] {
        let (base, calls, thread) = server(vec![response(status, headers, "{}")]);
        let mut client = GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap();
        assert!(client.repository("acme/sdk").await.is_err());
        assert!(client.repository("acme/core").await.is_err());
        assert_eq!(client.auth_failed.load(std::sync::atomic::Ordering::Relaxed),status.starts_with("401"));
        thread.join().unwrap();
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn discussion_snapshots_fetch_all_pages_and_human_review_sources() {
    let user = r#"{"id":7,"login":"alice","type":"User"}"#;
    let comment =
        format!(r#"[{{"id":1,"user":{user},"created_at":"2026-11-01T00:00:00Z","html_url":""}}]"#);
    let review = format!(
        r#"[{{"id":2,"user":{user},"submitted_at":"2026-11-02T00:00:00Z","state":"APPROVED","html_url":""}},{{"id":3,"user":{user},"state":"PENDING"}}]"#
    );
    let (base, calls, thread) = server(vec![
        response(
            "200 OK",
            "Link: <http://example.test?page=2>; rel=\"next\"\r\n",
            &comment,
        ),
        response("200 OK", "", "[]"),
        response("200 OK", "", &review),
        response("200 OK", "", "[]"),
    ]);
    let mut client = GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap();
    let events = client.engagement("acme/sdk", 1, "pr").await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["kind"], "review");
    thread.join().unwrap();
    assert!(calls.lock().unwrap()[1].contains("page=2"));
}
#[tokio::test]
async fn partial_graphql_response_is_rejected_instead_of_committed() {
    let (base, _, thread) = server(vec![response(
        "200 OK",
        "",
        r#"{"data":{},"errors":[{"message":"partial result"}]}"#,
    )]);
    let mut client = GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap();
    assert!(client.pull("acme/sdk", 1).await.is_err());
    thread.join().unwrap();
}

#[tokio::test]
async fn repository_preferences_validate_github_access_before_persisting_additions() {
    use tracker_core::{
        config::{Config, Repository},
        github::SharedGitHub,
        repository_config::RepositoryManager,
        store::Store,
    };
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::load(dir.path(), None).unwrap();
    let store = Store::open(&dir.path().join("tracker.sqlite"))
        .await
        .unwrap();
    store.configure(&cfg).await.unwrap();
    let manager =
        RepositoryManager::new(dir.path().join("repositories.json"), cfg.clone()).unwrap();
    let before = manager.settings().await;
    let mut repos = cfg.repositories.clone();
    repos.push(Repository {
        name: "acme/new".into(),
        include_terms: vec![],
        enabled: true,
    });
    let (base, _, thread) = server(vec![response("404 Not Found", "", "{}")]);
    let github =
        SharedGitHub::new(GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap());
    assert!(
        manager
            .save(&store, repos.clone(), &before.revision, Some(github))
            .await
            .is_err()
    );
    thread.join().unwrap();
    assert_eq!(manager.settings().await.revision, before.revision);
    let (base, calls, thread) = server(vec![response(
        "200 OK",
        "",
        r#"{"id":30,"full_name":"acme/new","html_url":"https://github.com/acme/new"}"#,
    )]);
    let github =
        SharedGitHub::new(GitHub::with_base("fake-token".into(), base, Duration::ZERO).unwrap());
    let saved = manager
        .save(&store, repos, &before.revision, Some(github))
        .await
        .unwrap();
    thread.join().unwrap();
    assert_eq!(saved.repositories.len(), cfg.repositories.len() + 1);
    assert!(calls.lock().unwrap()[0].contains("/repos/acme/new"));
    assert_eq!(
        store.repositories().await.unwrap().len(),
        saved.repositories.len()
    );
}

#[tokio::test]
async fn discussion_pages_cache_bodies_and_keep_saved_comments_when_refresh_fails() {
    use serde_json::json;
    use tracker_core::{discussion, github::SharedGitHub};
    let (base,calls,thread)=server(vec![
        response("200 OK", "Link: <http://example.test?page=2>; rel=\"next\"\r\n", r#"[{"id":10,"user":{"login":"alice"},"body":"Please try **this**","created_at":"2026-11-01T12:00:00Z","updated_at":"2026-11-02T12:00:00Z","html_url":"https://github.com/acme/sdk/issues/1#issuecomment-10"}]"#),
        response("200 OK", "", r#"[{"id":11,"user":null,"body":"Second page"}]"#),
        response("503 Service Unavailable", "", r#"{"message":"Unavailable"}"#),
    ]);
    let github=SharedGitHub::new(GitHub::with_base("fake-token".into(),base,Duration::ZERO).unwrap());
    let directory=tempfile::tempdir().unwrap();
    let mut item=json!({"repo":"acme/sdk","number":1,"updated_at":"2026-11-02T12:00:00Z"});
    let first=discussion::load(directory.path(),&item,1,false,Some(github.clone())).await.unwrap();
    assert_eq!(first["next"],2);assert_eq!(first["comments"][0]["body"],"Please try **this**");
    // A fresh page needs neither credentials nor a network request.
    assert_eq!(discussion::load(directory.path(),&item,1,false,None).await.unwrap(),first);
    let second=discussion::load(directory.path(),&item,2,false,Some(github.clone())).await.unwrap();
    assert!(second["next"].is_null());assert_eq!(second["comments"][0]["author"],"Unknown author");
    let failed=discussion::load(directory.path(),&item,1,true,Some(github)).await.unwrap();
    assert_eq!(failed["comments"],first["comments"]);assert!(failed["warning"].as_str().unwrap().contains("Showing saved comments"));
    item["updated_at"]=json!("2026-11-03T12:00:00Z");
    let offline=discussion::load(directory.path(),&item,1,false,None).await.unwrap();
    assert_eq!(offline["comments"],first["comments"]);assert!(offline["warning"].as_str().unwrap().contains("Offline"));
    assert!(discussion::load(directory.path(),&item,3,false,None).await.is_err());
    assert!(discussion::load(directory.path(),&item,0,false,None).await.is_err());
    thread.join().unwrap();
    let calls=calls.lock().unwrap();assert_eq!(calls.len(),3);
    assert!(calls[0].contains("/repos/acme/sdk/issues/1/comments?per_page=30&page=1"));
    assert!(calls[1].contains("page=2"));
}

#[tokio::test]
async fn maintainer_settings_resolve_usernames_verify_ids_and_keep_failed_saves_atomic(){
    use tracker_core::{maintainer_config::{MaintainerManager,write_file},config::Maintainer,store::Store,github::SharedGitHub};
    let (base,calls,thread)=server(vec![
        response("200 OK","",r#"{"id":123,"login":"Alice","type":"User"}"#),
        response("200 OK","",r#"{"id":456,"login":"bob","type":"User"}"#),
    ]);
    let github=SharedGitHub::new(GitHub::with_base("fake-token".into(),base,Duration::ZERO).unwrap());
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    let path=dir.path().join("maintainers.json");write_file(&path,&[]).unwrap();let manager=MaintainerManager::new(path.clone(),vec![]).unwrap();
    let initial=manager.settings().await;
    let saved=manager.save(&store,vec![Maintainer{github_id:String::new(),login:"@alice".into()}],&initial.revision,Some(github.clone())).await.unwrap();
    assert_eq!(saved.maintainers[0].github_id,"123");assert_eq!(saved.maintainers[0].login,"Alice");
    let raw=std::fs::read(&path).unwrap();
    assert!(manager.save(&store,vec![Maintainer{github_id:"999".into(),login:"bob".into()}],&saved.revision,Some(github)).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(),raw);assert_eq!(manager.settings().await.maintainers[0].github_id,"123");
    thread.join().unwrap();let calls=calls.lock().unwrap();assert_eq!(calls.len(),2);assert!(calls[0].contains("/users/alice"));
}

#[tokio::test]
async fn customer_settings_resolve_usernames_verify_ids_and_keep_failed_saves_atomic(){
    use tracker_core::{customer_config::{CustomerManager,write_file},customer_config::Customer,store::Store,github::SharedGitHub};
    let (base,calls,thread)=server(vec![
        response("200 OK","",r#"{"id":123,"login":"Alice","type":"User"}"#),
        response("200 OK","",r#"{"id":456,"login":"bob","type":"User"}"#),
    ]);
    let github=SharedGitHub::new(GitHub::with_base("fake-token".into(),base,Duration::ZERO).unwrap());
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    let path=dir.path().join("customers.json");write_file(&path,&[]).unwrap();let manager=CustomerManager::new(path.clone(),vec![]).unwrap();
    let initial=manager.settings().await;
    let saved=manager.save(&store,vec![Customer{github_id:String::new(),login:"@alice".into(),full_name:"Alice Example".into(),notes:"Acme".into(),..Default::default()}],&initial.revision,Some(github.clone())).await.unwrap();
    assert_eq!(saved.customers[0].github_id,"123");assert_eq!(saved.customers[0].login,"Alice");
    assert_eq!(saved.customers[0].full_name,"Alice Example");assert_eq!(saved.customers[0].notes,"Acme");
    let raw=std::fs::read(&path).unwrap();
    assert!(manager.save(&store,vec![Customer{github_id:"999".into(),login:"bob".into(),..Default::default()}],&saved.revision,Some(github)).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(),raw);assert_eq!(manager.settings().await.customers[0].github_id,"123");
    thread.join().unwrap();let calls=calls.lock().unwrap();assert_eq!(calls.len(),2);assert!(calls[0].contains("/users/alice"));
}

#[tokio::test]
async fn onboarding_validates_rest_and_graphql_without_writes() {
    let (base,calls,thread)=server(vec![
        response("200 OK","",r#"{"id":123,"login":"alice"}"#),
        response("200 OK","",r#"{"data":{"viewer":{"login":"alice"}}}"#),
    ]);
    let mut github=GitHub::with_base("fake-test-token".into(),base,Duration::ZERO).unwrap();
    github.validate_credentials().await.unwrap();thread.join().unwrap();
    let calls=calls.lock().unwrap();assert!(calls[0].starts_with("GET /user "));assert!(calls[1].starts_with("POST /graphql "));assert!(calls[1].contains("query { viewer { login } }"));assert!(!calls[1].contains("mutation"));
}

#[tokio::test]
async fn onboarding_rejects_invalid_tokens_and_unavailable_graphql() {
    let (base,_,thread)=server(vec![response("401 Unauthorized","",r#"{"message":"Bad credentials"}"#)]);
    let mut github=GitHub::with_base("fake-invalid".into(),base,Duration::ZERO).unwrap();
    assert!(github.validate_credentials().await.is_err());thread.join().unwrap();
    let (base,_,thread)=server(vec![response("200 OK","",r#"{"id":123,"login":"alice"}"#),response("200 OK","",r#"{"data":{"viewer":null},"errors":[{"message":"Denied"}]}"#)]);
    let mut github=GitHub::with_base("fake-limited".into(),base,Duration::ZERO).unwrap();
    assert!(github.validate_credentials().await.is_err());thread.join().unwrap();
}

#[tokio::test]
async fn replacement_token_recovers_all_shared_github_consumers() {
    use tracker_core::github::SharedGitHub;
    use std::sync::atomic::Ordering;
    let (base,_,thread)=server(vec![response("401 Unauthorized","","{}")]);
    let client=GitHub::with_base("expired".into(),base,Duration::ZERO).unwrap();
    let failed=client.auth_failed.clone();
    let shared=SharedGitHub::new(client);let mut consumer=shared.clone();
    assert!(consumer.repository("acme/sdk").await.is_err());thread.join().unwrap();assert!(failed.load(Ordering::Relaxed));
    let (base,calls,thread)=server(vec![response("200 OK","",r#"{"id":1,"full_name":"acme/sdk"}"#)]);
    shared.replace(GitHub::with_base("replacement".into(),base,Duration::ZERO).unwrap()).await;
    assert!(!failed.load(Ordering::Relaxed));assert!(consumer.repository("acme/sdk").await.is_ok());
    thread.join().unwrap();assert!(calls.lock().unwrap()[0].to_lowercase().contains("authorization: bearer replacement"));
}

#[tokio::test]
async fn developer_search_paginates_and_rejects_incomplete_results(){
    let (base,calls,thread)=server(vec![
        response("200 OK","Link: <http://example.test?page=2>; rel=\"next\"\r\n",r#"{"total_count":101,"incomplete_results":false,"items":[{"id":1}]}"#),
        response("200 OK","",r#"{"total_count":101,"incomplete_results":false,"items":[{"id":2}]}"#),
        response("200 OK","",r#"{"total_count":1001,"incomplete_results":false,"items":[]}"#),
        response("200 OK","",r#"{"total_count":5,"incomplete_results":true,"items":[]}"#),
    ]);
    let mut github=GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap();
    assert_eq!(github.search_issues("repo:acme/sdk mentions:alice",1).await.unwrap().next,Some(2));
    assert_eq!(github.search_issues("repo:acme/sdk mentions:alice",2).await.unwrap().items[0]["id"],2);
    assert!(github.search_issues("repo:acme/sdk author:alice",1).await.is_err());
    assert!(github.search_issues("repo:acme/sdk commenter:alice",1).await.is_err());
    thread.join().unwrap();assert!(calls.lock().unwrap()[0].contains("repo%3Aacme%2Fsdk+mentions%3Aalice"));
}

#[tokio::test]
async fn developer_work_bypasses_followup_filters_shares_notes_and_preserves_complete_cache(){
    use serde_json::json;
    use tracker_core::{developer,store::Store,github::SharedGitHub,config::Config};
    let item=json!({"id":123,"number":4,"node_id":"PR_4","title":"Unrelated title","body":"Discussion","state":"open","user":{"id":7,"login":"alice","type":"User"},"assignees":[{"id":7,"login":"alice"}],"labels":[],"comments":1,"pull_request":{},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-11-01T00:00:00Z","html_url":"https://github.com/acme/sdk/pull/4"});
    let result=json!({"total_count":1,"incomplete_results":false,"items":[item]}).to_string();
    let responses=vec![
        response("200 OK","",r#"{"requested_reviewers":[{"login":"alice"}]}"#),
        response("200 OK","",r#"[{"event":"review_requested","requested_reviewer":{"login":"alice"},"created_at":"2026-11-10T00:00:00Z"}]"#),
        response("200 OK","",r#"[{"event":"assigned","assignee":{"login":"alice"},"created_at":"2026-11-02T00:00:00Z"}]"#),
        response("200 OK","",&result),response("200 OK","",&result),
        response("503 Unavailable","","{}"),response("503 Unavailable","","{}"),
    ];
    let (base,calls,thread)=server(responses);
    let github=SharedGitHub::new(GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap());
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    let mut config:Config=serde_json::from_value(json!({"repositories":[{"name":"acme/sdk","includeTerms":["powersync"]},{"name":"acme/disabled","enabled":false}],"maintainers":[{"githubId":"7","login":"alice"}]})).unwrap();
    store.configure(&config).await.unwrap();
    let repo=store.save_repository("acme/sdk",&json!({"id":10,"full_name":"acme/sdk"})).await.unwrap();
    store.save_items(&repo,&[item],&Default::default()).await.unwrap();
    store.update_poll("10",json!({"stage":"ready","last_success":tracker_core::store::now()})).await.unwrap();
    let first=developer::load(dir.path(),&store,Some(github.clone()),"Alice",false,false).await.unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(),1);let entry=&first["items"][0];
    assert_eq!(entry["developerReasons"].as_array().unwrap().len(),5);
    assert_eq!(entry["actionTimes"]["review"],"2026-11-10T00:00:00Z");assert_eq!(entry["actionTimes"]["assigned"],"2026-11-02T00:00:00Z");
    assert_eq!(store.followup().await.unwrap()["total"],0);
    store.advice("123","Shared note",true).await.unwrap();
    let cached=developer::load(dir.path(),&store,None,"alice",false,false).await.unwrap();assert_eq!(cached["items"][0]["advice"]["text"],"Shared note");
    let failed=developer::load_progress(dir.path(),&store,Some(github),"alice",false,true,"participating",&|_|{}).await.unwrap();assert_eq!(failed["items"].as_array().unwrap().len(),1);assert!(failed["warning"].as_str().unwrap().contains("saved results"));
    config.repositories[0].enabled=false;store.configure(&config).await.unwrap();
    assert!(developer::load(dir.path(),&store,None,"alice",false,false).await.unwrap()["items"].as_array().unwrap().is_empty());
    thread.join().unwrap();assert!(calls.lock().unwrap().iter().all(|call|!call.contains("acme%2Fdisabled")));
    assert!(developer::validate_login("alice is:closed").is_err());
}

#[tokio::test]
async fn discussion_includes_review_summaries_with_independent_pagination(){
    use tracker_core::{discussion,github::SharedGitHub};
    let (base,_,thread)=server(vec![
        response("200 OK","",r#"[{"id":7,"user":{"login":"alice"},"body":"Comment","created_at":"2026-11-01T00:00:00Z"}]"#),
        response("200 OK","Link: <http://example.test?page=2>; rel=\"next\"\r\n",r#"[{"id":7,"user":{"login":"alice"},"body":"Looks good","state":"APPROVED","submitted_at":"2026-11-02T00:00:00Z"}]"#),
        response("200 OK","","[]"),
        response("200 OK","",r#"[{"id":8,"user":{"login":"bob"},"body":"Draft","state":"PENDING"},{"id":9,"user":{"login":"bob"},"body":"Please adjust","state":"CHANGES_REQUESTED","submitted_at":"2026-11-03T00:00:00Z"}]"#),
    ]);
    let github=SharedGitHub::new(GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap());let dir=tempfile::tempdir().unwrap();
    let item=serde_json::json!({"repo":"acme/sdk","number":4,"type":"pr","updated_at":"2026-11-03T00:00:00Z"});
    let first=discussion::load(dir.path(),&item,1,false,Some(github.clone())).await.unwrap();assert_eq!(first["comments"].as_array().unwrap().len(),2);assert_eq!(first["next"],2);assert_eq!(first["comments"][1]["kind"],"review");
    let second=discussion::load(dir.path(),&item,2,false,Some(github)).await.unwrap();assert_eq!(second["comments"].as_array().unwrap().len(),1);assert_eq!(second["comments"][0]["review_state"],"CHANGES_REQUESTED");assert!(second["next"].is_null());thread.join().unwrap();
}

#[tokio::test]
async fn developer_queue_navigation_reuses_searches_and_persisted_cooldown(){
    use tracker_core::{developer,store::Store,github::SharedGitHub,config::Config};
    use serde_json::json;
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    let config:Config=serde_json::from_value(json!({"repositories":[{"name":"acme/sdk"}]})).unwrap();store.configure(&config).await.unwrap();store.save_repository("acme/sdk",&json!({"id":10,"full_name":"acme/sdk"})).await.unwrap();
    store.update_poll("10",json!({"stage":"ready","last_success":tracker_core::store::now()})).await.unwrap();
    let empty=r#"{"total_count":0,"incomplete_results":false,"items":[]}"#;
    let (base,calls,thread)=server(vec![response("200 OK","",empty),response("200 OK","",empty),response("429 Too Many Requests","Retry-After: 600\r\n","{}")]);
    let github=SharedGitHub::new(GitHub::with_base("fake".into(),base.clone(),Duration::ZERO).unwrap());
    let created=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,false,"created",&|_|{}).await.unwrap();
    assert_eq!(calls.lock().unwrap().len(),0);assert!(created["loaded_queues"].as_array().unwrap().contains(&json!("created")));
    developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,false,"assigned",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),0);
    let participation=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,false,"participating",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),2);assert!(participation["loaded_queues"].as_array().unwrap().contains(&json!("participating")));
    developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,false,"participating",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),2);
    let startup=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,true,"cached",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),2);assert!(startup["loaded_queues"].as_array().unwrap().contains(&json!("participating")));
    let other_profile=developer::load_progress(dir.path(),&store,Some(github.clone()),"bob",false,true,"cached",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),2);assert!(!other_profile["loaded_queues"].as_array().unwrap().contains(&json!("participating")));

    let limited=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,true,"participating",&|_|{}).await.unwrap();assert_eq!(limited["paused"],true);
    let forced=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,true,"assigned",&|_|{}).await.unwrap();assert_eq!(forced["paused"],true);assert!(forced["loaded_queues"].as_array().unwrap().contains(&json!("assigned")));
    let restarted=SharedGitHub::new(GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap());
    let other=developer::load_progress(dir.path(),&store,Some(restarted),"bob",true,true,"created",&|_|{}).await.unwrap();assert_eq!(other["paused"],true);assert_eq!(calls.lock().unwrap().len(),3);
    thread.join().unwrap();let calls=calls.lock().unwrap();assert!(calls[0].contains("mentions%3Aalice"));assert!(calls[1].contains("commenter%3Aalice"));

}

#[tokio::test]
async fn shared_client_restores_importer_cooldown_before_network_access(){
 let directory=tempfile::tempdir().unwrap();let path=directory.path().join("cooldown.json");
 let (base,calls,thread)=server(vec![response("429 Too Many Requests","Retry-After: 600\r\n","{}")]);
 let mut first=GitHub::with_base("fake".into(),base.clone(),Duration::ZERO).unwrap();first.restore_cooldown(path.clone());assert!(first.repository("acme/sdk").await.is_err());
 assert!(path.exists());let mut restarted=GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap();restarted.restore_cooldown(path);assert!(restarted.repository("acme/sdk").await.is_err());assert_eq!(calls.lock().unwrap().len(),1);thread.join().unwrap();
}

#[tokio::test]
async fn local_developer_membership_tracks_import_changes_and_coverage() {
    use serde_json::json;
    use tracker_core::{developer,store::Store,config::Config};
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    let config:Config=serde_json::from_value(json!({"repositories":[{"name":"acme/sdk","includeTerms":["powersync"]}]})).unwrap();
    store.configure(&config).await.unwrap();let repo=store.save_repository("acme/sdk",&json!({"id":10,"full_name":"acme/sdk"})).await.unwrap();
    let mut item=json!({"id":123,"number":4,"title":"Other integration","state":"open","user":{"id":7,"login":"Alice","type":"User"},"assignees":[{"id":8,"login":"bob"}],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-11-01T00:00:00Z"});
    store.save_items(&repo,&[item.clone()],&Default::default()).await.unwrap();
    let partial=developer::load_progress(dir.path(),&store,None,"alice",false,false,"created",&|_|{}).await.unwrap();
    assert_eq!(partial["items"][0]["id"],"123");assert!(!partial["loaded_queues"].as_array().unwrap().contains(&json!("created")));
    store.update_poll("10",json!({"stage":"backfill","last_success":tracker_core::store::now()})).await.unwrap();
    let assigned=developer::load_progress(dir.path(),&store,None,"bob",false,false,"assigned",&|_|{}).await.unwrap();assert_eq!(assigned["items"][0]["developerReasons"],json!(["assigned"]));assert!(assigned["loaded_queues"].as_array().unwrap().contains(&json!("assigned")));
    let history=developer::load_progress(dir.path(),&store,None,"alice",true,false,"created",&|_|{}).await.unwrap();assert!(!history["loaded_queues"].as_array().unwrap().contains(&json!("created")));
    item["assignees"]=json!([]);item["state"]=json!("closed");item["updated_at"]=json!("2026-11-02T00:00:00Z");store.save_items(&repo,&[item],&Default::default()).await.unwrap();
    let removed=developer::load_progress(dir.path(),&store,None,"bob",false,false,"assigned",&|_|{}).await.unwrap();assert!(removed["items"].as_array().unwrap().is_empty());
    let closed=developer::load_progress(dir.path(),&store,None,"alice",false,false,"created",&|_|{}).await.unwrap();assert!(closed["items"].as_array().unwrap().is_empty());
    let history=developer::load_progress(dir.path(),&store,None,"alice",true,false,"created",&|_|{}).await.unwrap();assert_eq!(history["items"][0]["id"],"123");
    store.save_engagement("123",&[json!({"id":1,"kind":"comment","user":{"id":8,"login":"bob","type":"User"},"at":"2026-11-02T00:00:00Z"})],"2026-11-02T00:00:00Z").await.unwrap();
    let local=developer::load_progress(dir.path(),&store,None,"bob",true,false,"created",&|_|{}).await.unwrap();assert_eq!(local["items"][0]["developerReasons"],json!(["commented"]));
    let folder=dir.path().join("discussions/repo-acme/repo-sdk");std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("4-1-reviews-v1.json"),json!({"version":"2026-11-02T00:00:00Z","data":{"comments":[{"author":"carol"}]}}).to_string()).unwrap();
    let discussion=developer::load_progress(dir.path(),&store,None,"carol",true,false,"created",&|_|{}).await.unwrap();assert_eq!(discussion["items"][0]["developerReasons"],json!(["commented"]));

}

#[tokio::test]
async fn imported_reviewers_are_shared_across_profiles_and_invalidated_by_updates() {
    use serde_json::json;
    use tracker_core::{developer,store::Store,config::Config,github::SharedGitHub};
    let dir=tempfile::tempdir().unwrap();let store=Store::open(&dir.path().join("db.sqlite")).await.unwrap();
    store.configure(&serde_json::from_value::<Config>(json!({"repositories":[{"name":"acme/sdk"}]})).unwrap()).await.unwrap();
    let repo=store.save_repository("acme/sdk",&json!({"id":10,"full_name":"acme/sdk"})).await.unwrap();
    store.update_poll("10",json!({"stage":"ready","last_success":tracker_core::store::now()})).await.unwrap();
    let mut item=json!({"id":123,"number":4,"title":"PR","state":"open","user":{"id":7,"login":"other","type":"User"},"pull_request":{},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-11-01T00:00:00Z"});
    let pulls=std::collections::HashMap::from([(4,json!({"requested_reviewers":[{"login":"alice"}],"closing":[]}))]);
    store.save_items(&repo,&[item.clone()],&pulls).await.unwrap();
    let (base,calls,thread)=server(vec![response("200 OK","","[]"),response("200 OK","",r#"{"requested_reviewers":[]}"#)]);
    let github=SharedGitHub::new(GitHub::with_base("fake".into(),base,Duration::ZERO).unwrap());
    let first=developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,false,"review",&|_|{}).await.unwrap();assert_eq!(first["items"][0]["developerReasons"],json!(["review"]));
    developer::load_progress(dir.path(),&store,Some(github.clone()),"alice",false,true,"review",&|_|{}).await.unwrap();
    developer::load_progress(dir.path(),&store,Some(github.clone()),"bob",false,false,"review",&|_|{}).await.unwrap();assert_eq!(calls.lock().unwrap().len(),1); // only Alice's action timeline
    assert!(!store.should_enrich("acme/sdk",&item).await.unwrap());
    item["updated_at"]=json!("2026-11-02T00:00:00Z");store.save_items(&repo,&[item.clone()],&Default::default()).await.unwrap();assert!(store.should_enrich("acme/sdk",&item).await.unwrap());
    let removed=developer::load_progress(dir.path(),&store,Some(github),"alice",false,false,"review",&|_|{}).await.unwrap();assert!(removed["items"].as_array().unwrap().is_empty());thread.join().unwrap();assert_eq!(calls.lock().unwrap().len(),2);
}

use crate::config::Config;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Months, Utc};
use powersync::{ConnectionPool, PowerSyncDatabase, env::PowerSyncEnvironment};
use rusqlite::{
    Connection, params, params_from_iter,
    types::{Value as Sql, ValueRef},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn id(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
pub fn sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or_default())),
        Value::String(s) => Sql::Text(s.clone()),
        _ => Sql::Text(v.to_string()),
    }
}
pub fn rows(conn: &Connection, query: &str, args: &[Value]) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(query)?;
    let names = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let args = args.iter().map(sql).collect::<Vec<_>>();
    let mut rows = stmt.query(params_from_iter(args))?;
    let mut result = vec![];
    while let Some(row) = rows.next()? {
        let mut object = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            object.insert(
                name.clone(),
                match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => json!(std::str::from_utf8(s)?),
                    ValueRef::Blob(_) => Value::Null,
                },
            );
        }
        result.push(Value::Object(object));
    }
    Ok(result)
}
pub fn upsert(conn: &Connection, table: &str, value: &Value) -> Result<()> {
    let map = value.as_object().context("Expected SQL row")?;
    // Table/column identifiers come exclusively from our schema and row constructors.
    let keys = map.keys().collect::<Vec<_>>();
    let query = format!(
        "INSERT OR REPLACE INTO {table} ({}) VALUES ({})",
        keys.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(","),
        vec!["?"; keys.len()].join(",")
    );
    conn.execute(&query, params_from_iter(keys.iter().map(|k| sql(&map[*k]))))?;
    Ok(())
}
pub struct Store {
    pub db: PowerSyncDatabase,
    pub revision: AtomicU64,
}
impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        PowerSyncEnvironment::powersync_auto_extension()?;
        let pool = ConnectionPool::open(path)?;
        let env = PowerSyncEnvironment::custom(
            reqwest::Client::new(),
            pool,
            PowerSyncEnvironment::tokio(),
        );
        let schema =
            serde_json::value::RawValue::from_string(include_str!("schema.json").to_owned())?;
        let db = PowerSyncDatabase::new(env, schema.as_ref());
        {
            let conn = db.writer().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS tracker_meta (key TEXT PRIMARY KEY,value TEXT); CREATE TABLE IF NOT EXISTS tracker_pr_reviewers (id TEXT PRIMARY KEY,version TEXT,reviewers TEXT,full_details INTEGER DEFAULT 0); CREATE TABLE IF NOT EXISTS tracker_action_times (id TEXT,login TEXT,reason TEXT,version TEXT,times TEXT,PRIMARY KEY(id,login,reason))",
            )?;
        }
        Ok(Self {
            db,
            revision: AtomicU64::new(1),
        })
    }
    pub fn changed(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
    }
    pub async fn query(&self, query: &str, args: &[Value]) -> Result<Vec<Value>> {
        let conn = self.db.reader().await?;
        rows(&conn, query, args)
    }
    pub async fn one(&self, query: &str, args: &[Value]) -> Result<Value> {
        Ok(self
            .query(query, args)
            .await?
            .into_iter()
            .next()
            .unwrap_or(Value::Null))
    }
    pub async fn execute(&self, query: &str, args: &[Value]) -> Result<()> {
        let conn = self.db.writer().await?;
        conn.execute(query, params_from_iter(args.iter().map(sql)))?;
        drop(conn);
        self.changed();
        Ok(())
    }
    pub async fn import_legacy(&self, path: &Path) -> Result<usize> {
        let mut conn = self.db.writer().await?;
        if conn.query_row(
            "SELECT count(*) FROM tracker_meta WHERE key='legacy_import'",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0
        {
            return Ok(0);
        }
        if conn.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))? > 0 {
            return Ok(0);
        }
        if !path.exists() {
            return Ok(0);
        }
        let source = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .context("Cannot read legacy database")?;
        source.busy_timeout(std::time::Duration::from_secs(10))?;
        source.execute_batch("BEGIN")?;
        let schema: Value = serde_json::from_str(include_str!("schema.json"))?;
        let tx = conn.transaction()?;
        let mut count = 0;
        for table in schema["tables"].as_array().unwrap() {
            let name = string(table, "name");
            if source.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name=?",
                [name],
                |r| r.get::<_, i64>(0),
            )? == 0
            {
                continue;
            }
            for row in rows(&source, &format!("SELECT * FROM {name}"), &[])? {
                if name == "items" {
                    count += 1;
                }
                upsert(&tx, name, &row)?;
            }
        }
        tx.execute(
            "INSERT INTO tracker_meta(key,value) VALUES ('legacy_import',?)",
            [now()],
        )?;
        tx.commit()?;
        source.execute_batch("COMMIT")?;
        self.changed();
        Ok(count)
    }
    pub async fn configure_repositories(&self, config: &Config) -> Result<()> {
        let mut conn = self.db.writer().await?;
        let tx = conn.transaction()?;
        let title_migration=rows(&tx,"SELECT value FROM tracker_meta WHERE key='repository_filter_version'",&[])?.first().is_none_or(|r|r["value"]!="title-v1");
        tx.execute("UPDATE repositories SET enabled=0", [])?;
        for (position, repo) in config.repositories.iter().enumerate() {
            let old = rows(
                &tx,
                "SELECT * FROM repositories WHERE configured_name=? OR name=?",
                &[json!(repo.name), json!(repo.name)],
            )?
            .into_iter()
            .next();
            let mut value=old.unwrap_or_else(||json!({"id":format!("pending:{}",repo.name),"name":repo.name,"url":format!("https://github.com/{}",repo.name)}));
            value["configured_name"] = json!(repo.name);
            let terms_changed = value["include_terms"].as_str() != Some(&serde_json::to_string(&repo.include_terms)?);
            value["include_terms"] = json!(serde_json::to_string(&repo.include_terms)?);
            value["enabled"] = json!(repo.enabled);
            value["position"] = json!(position);
            upsert(&tx, "repositories", &value)?;
            if terms_changed || title_migration {
                for item in rows(&tx,"SELECT id,title,body,ever_included FROM items WHERE repo_id=?",&[value["id"].clone()])? {
                    let included=crate::config::includes(&repo.include_terms,string(&item,"title"));
                    tx.execute("UPDATE items SET included=?,ever_included=? WHERE id=?",params![included,included || item["ever_included"]!=0,string(&item,"id")])?;
                }
                // Re-enrich newly included historical PRs during the next reconciliation.
                tx.execute("UPDATE poll_state SET last_reconcile=NULL,reconcile_page=NULL,reconcile_started=NULL WHERE id=?",params![string(&value,"id")])?;
            }
        }
        tx.execute("INSERT OR REPLACE INTO tracker_meta(key,value) VALUES ('repository_filter_version','title-v1')",[])?;
        tx.commit()?;self.changed();Ok(())
    }
    pub async fn configure_maintainers(&self, maintainers: &[crate::config::Maintainer]) -> Result<()> {
        let mut conn=self.db.writer().await?;let tx=conn.transaction()?;
        tx.execute("DELETE FROM maintainers", [])?;
        for person in maintainers {upsert(&tx,"maintainers",&json!({"id":person.github_id,"login":person.login}))?;}
        tx.commit()?;self.changed();Ok(())
    }
    pub async fn configure_customers(&self, customers: &[crate::customer_config::Customer]) -> Result<()> {
        let mut conn=self.db.writer().await?;let tx=conn.transaction()?;
        tx.execute("DELETE FROM customers", [])?;
        for person in customers {upsert(&tx,"customers",&json!({"id":person.github_id,"login":person.login,"full_name":person.full_name,"company":person.notes,"vip_company":person.company,"highlight":serde_json::to_value(person.highlight)?}))?;}
        tx.commit()?;self.changed();Ok(())
    }
    pub async fn configure(&self, config: &Config) -> Result<()> {
        self.execute("UPDATE poll_state SET active=0 WHERE active=1",&[]).await?;
        self.configure_repositories(config).await?;
        self.configure_maintainers(&config.maintainers).await?;
        let mut conn = self.db.writer().await?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM identities", [])?;
        for person in &config.people {
            upsert(
                &tx,
                "identities",
                &json!({"id":person.github_id,"internal":person.internal,"customers":serde_json::to_string(&person.customers)?}),
            )?;
        }
        tx.commit()?;
        self.changed();
        Ok(())
    }
    pub async fn repositories(&self) -> Result<Vec<Value>> {
        self.query(
            "SELECT * FROM repositories WHERE enabled=1 ORDER BY position",
            &[],
        )
        .await
    }
    pub async fn repository(&self, name: &str) -> Result<Value> {
        let value = self
            .one(
                "SELECT * FROM repositories WHERE configured_name=? OR name=?",
                &[json!(name), json!(name)],
            )
            .await?;
        if value.is_null() {
            bail!("Repository not configured: {name}");
        }
        Ok(value)
    }
    pub async fn save_repository(&self, name: &str, meta: &Value) -> Result<Value> {
        let old = self.repository(name).await?;
        let key = id(&meta["id"]);
        let canonical = string(meta, "full_name").to_lowercase();
        let mut conn = self.db.writer().await?;
        let tx = conn.transaction()?;
        if string(&old, "id") != key {
            tx.execute("DELETE FROM repositories WHERE id=?", [string(&old, "id")])?;
        }
        let mut updated = old.clone();
        updated["id"] = json!(key);
        updated["name"] = json!(canonical);
        updated["url"] = meta["html_url"].clone();
        upsert(&tx, "repositories", &updated)?;
        tx.execute("INSERT OR IGNORE INTO poll_state (id,stage,open_page,backfill_page,initial_at,failures) VALUES (?,'opening',1,1,?,0)",params![key,now()])?;
        if string(&old, "name") != canonical {
            tx.execute(
                "UPDATE relationships SET target_repo=? WHERE target_repo=?",
                params![canonical, string(&old, "name")],
            )?;
        }
        tx.commit()?;
        self.changed();
        Ok(updated)
    }
    pub async fn poll_state(&self, key: &str) -> Result<Value> {
        self.one("SELECT * FROM poll_state WHERE id=?", &[json!(key)])
            .await
    }
    pub async fn update_poll(&self, key: &str, changes: Value) -> Result<()> {
        let mut state = self.poll_state(key).await?;
        if state.is_null() {
            state = json!({"id":key,"stage":"opening","open_page":1,"backfill_page":1,"initial_at":now(),"failures":0});
        }
        for (k, v) in changes.as_object().context("Invalid poll state")? {
            state[k] = v.clone();
        }
        let conn = self.db.writer().await?;
        upsert(&conn, "poll_state", &state)?;
        drop(conn);
        self.changed();
        Ok(())
    }
    pub async fn save_items(
        &self,
        repo: &Value,
        items: &[Value],
        pulls: &std::collections::HashMap<u64, Value>,
    ) -> Result<()> {
        let mut conn = self.db.writer().await?;
        let tx = conn.transaction()?;
        // Read the current filter under the writer lock, so an in-flight poll cannot undo a settings change.
        let current=rows(&tx,"SELECT include_terms FROM repositories WHERE id=?",&[repo["id"].clone()])?.into_iter().next().unwrap_or_default();
        let terms:Vec<String>=serde_json::from_str(current["include_terms"].as_str().unwrap_or("[]"))?;
        for item in items {
            let key = id(&item["id"]);
            let previous = rows(
                &tx,
                "SELECT updated_at,ever_included,merged_at FROM items WHERE id=?",
                &[json!(key)],
            )?
            .into_iter()
            .next();
            if previous.as_ref().is_some_and(|p| string(&p, "updated_at") > string(item, "updated_at")) {
                tx.execute(
                    "UPDATE items SET last_seen=?,unavailable=0 WHERE id=?",
                    params![now(), key],
                )?;
                continue;
            }
            save_author(&tx, &item["user"])?;
            for user in item["assignees"].as_array().into_iter().flatten() {
                save_author(&tx, user)?;
            }
            let pull = pulls.get(&item["number"].as_u64().unwrap_or_default());
            let pr = !item["pull_request"].is_null();
            let included=crate::config::includes(&terms,string(item,"title"));
            let ever_included=included || previous.as_ref().is_some_and(|p|p["ever_included"]==1);
            let mut merged = pull
                .map(|p| p["mergedAt"].clone())
                .filter(|v| !v.is_null())
                .unwrap_or(item["pull_request"]["merged_at"].clone());
            if merged.is_null() && pull.is_none() {
                if let Some(previous)=&previous {merged=previous["merged_at"].clone();}
            }
            let state = if !merged.is_null() {
                "merged"
            } else if string(item, "state") == "closed" {
                "closed"
            } else if pull
                .map(|p| p["isDraft"].as_bool().unwrap_or(false))
                .unwrap_or(item["draft"].as_bool().unwrap_or(false))
            {
                "draft"
            } else {
                "open"
            };
            upsert(
                &tx,
                "items",
                &json!({"id":key,"included":included,"ever_included":ever_included,"repo_id":repo["id"],"node_id":item["node_id"],"number":item["number"],"type":if pr{"pr"}else{"issue"},"title":item["title"],"body":item["body"].as_str().unwrap_or(""),"state":state,"state_reason":item["state_reason"],"author_id":if item["user"].is_null(){Value::Null}else{json!(id(&item["user"]["id"]))},"labels":serde_json::to_string(&item["labels"].as_array().cloned().unwrap_or_default())?,"assignees":serde_json::to_string(&item["assignees"].as_array().cloned().unwrap_or_default())?,"comments":item["comments"],"created_at":item["created_at"],"updated_at":item["updated_at"],"closed_at":item["closed_at"],"merged_at":merged,"url":item["html_url"],"unavailable":0,"last_seen":now()}),
            )?;
            tx.execute(
                "DELETE FROM relationships WHERE source_id=? AND kind='reference'",
                [&key],
            )?;
            for reference in references(item["body"].as_str().unwrap_or(""), string(repo, "name")) {
                if reference["repo"] == repo["name"] && reference["number"] == item["number"] {
                    continue;
                }
                upsert(
                    &tx,
                    "relationships",
                    &json!({"id":format!("{key}:reference:{}#{}",string(&reference,"repo"),reference["number"]),"source_id":key,"target_repo":reference["repo"],"target_number":reference["number"],"target_url":reference["url"],"kind":"reference"}),
                )?;
            }
            if let Some(pull) = pull {
                if let Some(reviewers) = pull["requested_reviewers"].as_array() {
                    tx.execute("INSERT OR REPLACE INTO tracker_pr_reviewers(id,version,reviewers,full_details) VALUES(?,?,?,1)",params![key,string(item,"updated_at"),serde_json::to_string(reviewers)?])?;
                }
                tx.execute(
                    "DELETE FROM relationships WHERE source_id=? AND kind='closing'",
                    [&key],
                )?;
                for link in pull["closing"].as_array().into_iter().flatten() {
                    let repo = string(&link["repository"], "nameWithOwner").to_lowercase();
                    upsert(
                        &tx,
                        "relationships",
                        &json!({"id":format!("{key}:closing:{repo}#{}",link["number"]),"source_id":key,"target_repo":repo,"target_number":link["number"],"target_url":link["url"],"kind":"closing"}),
                    )?;
                }
            }
        }
        tx.commit()?;
        self.changed();
        Ok(())
    }
    pub async fn save_engagement(&self, key: &str, events: &[Value], observed: &str) -> Result<()> {
        let mut conn = self.db.writer().await?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM engagement_events WHERE item_id=?", [key])?;
        for event in events {
            if !human(&event["user"]) || DateTime::parse_from_rfc3339(string(event, "at")).is_err()
            {
                continue;
            }
            save_author(&tx, &event["user"])?;
            upsert(
                &tx,
                "engagement_events",
                &json!({"id":format!("{key}:{}:{}",string(event,"kind"),id(&event["id"])),"item_id":key,"kind":event["kind"],"occurred_at":normalize(string(event,"at")),"author_id":id(&event["user"]["id"]),"url":event["url"]}),
            )?;
        }
        upsert(
            &tx,
            "engagement_sync",
            &json!({"id":key,"synced_at":now(),"observed_updated_at":observed,"last_attempt":now(),"error":null,"next_retry":null,"failures":0}),
        )?;
        tx.commit()?;
        self.changed();
        Ok(())
    }
    pub async fn engagement_failure(&self, key: &str, message: &str, retry: &str) -> Result<()> {
        let mut old = self
            .one("SELECT * FROM engagement_sync WHERE id=?", &[json!(key)])
            .await?;
        if old.is_null() {
            old = json!({"id":key});
        }
        old["failures"] = json!(old["failures"].as_u64().unwrap_or(0) + 1);
        old["error"] = json!(message);
        old["last_attempt"] = json!(now());
        old["next_retry"] = json!(retry);
        let conn = self.db.writer().await?;
        upsert(&conn, "engagement_sync", &old)?;
        drop(conn);
        self.changed();
        Ok(())
    }
        pub async fn should_enrich(&self, repo: &str, item: &Value) -> Result<bool> {
        let cached=self.one("SELECT version FROM tracker_pr_reviewers WHERE id=? AND full_details=1",&[json!(id(&item["id"]))]).await?;
        if cached["version"].is_string() && cached["version"]==item["updated_at"] {return Ok(false);}
        let current=self.repository(repo).await?;
        let terms:Vec<String>=serde_json::from_str(current["include_terms"].as_str().unwrap_or("[]"))?;
        if crate::config::includes(&terms,string(item,"title")) {return Ok(true);}
        Ok(self.one("SELECT ever_included FROM items WHERE id=?",&[json!(id(&item["id"]))]).await?["ever_included"]==1)
    }
    pub async fn action_times(&self,id:&str,login:&str,reason:&str,version:&str)->Result<Option<std::collections::BTreeMap<String,String>>> {
        let row=self.one("SELECT times FROM tracker_action_times WHERE id=? AND login=? AND reason=? AND version=?",&[json!(id),json!(login),json!(reason),json!(version)]).await?;
        Ok(row["times"].as_str().and_then(|s|serde_json::from_str(s).ok()))
    }
    pub async fn save_action_times(&self,id:&str,login:&str,reason:&str,version:&str,times:&std::collections::BTreeMap<String,String>)->Result<()> {
        self.execute("INSERT OR REPLACE INTO tracker_action_times(id,login,reason,version,times) VALUES(?,?,?,?,?)",&[json!(id),json!(login),json!(reason),json!(version),json!(serde_json::to_string(times)?)]).await
    }
    pub async fn developer_items(&self, repo: &str, closed: bool) -> Result<Vec<Value>> {
        self.query("SELECT i.*,a.login,pr.version AS reviewer_version,pr.reviewers FROM items i JOIN repositories r ON r.id=i.repo_id LEFT JOIN authors a ON a.id=i.author_id LEFT JOIN tracker_pr_reviewers pr ON pr.id=i.id WHERE r.enabled=1 AND r.name=? AND coalesce(i.unavailable,0)=0 AND (? OR i.state IN ('open','draft'))", &[json!(repo),json!(closed)]).await
    }
    pub async fn save_reviewers(&self, id: &str, version: &str, reviewers: &Value) -> Result<()> {
        if !reviewers.is_array() {anyhow::bail!("Invalid requested reviewers response");}
        self.execute("INSERT OR REPLACE INTO tracker_pr_reviewers(id,version,reviewers) VALUES(?,?,?)", &[json!(id),json!(version),json!(serde_json::to_string(reviewers)?)]).await
    }
    pub async fn candidates(
        &self,
        limit: usize,
        seconds: u64,
        repo: Option<&str>,
    ) -> Result<Vec<Value>> {
        self.query("SELECT i.id,r.name AS repo,i.number,i.type,i.updated_at FROM items i JOIN repositories r ON r.id=i.repo_id LEFT JOIN authors a ON a.id=i.author_id LEFT JOIN engagement_sync es ON es.id=i.id WHERE r.enabled=1 AND coalesce(i.included,1)=1 AND i.state IN ('open','draft') AND coalesce(a.type,'User')!='Bot' AND coalesce(a.login,'') NOT LIKE '%[bot]' AND NOT EXISTS(SELECT 1 FROM maintainers m WHERE m.id=i.author_id) AND (? IS NULL OR r.name=?) AND (es.next_retry IS NULL OR julianday(es.next_retry)<=julianday('now')) AND (es.synced_at IS NULL OR julianday(es.synced_at)<=julianday('now',?) OR julianday(es.observed_updated_at)<julianday(i.updated_at)) ORDER BY es.synced_at IS NOT NULL,coalesce(es.last_attempt,''),i.id LIMIT ?",&[json!(repo),json!(repo),json!(format!("-{seconds} seconds")),json!(limit)]).await
    }
    pub async fn followup(&self) -> Result<Value> {
        let conn = self.db.reader().await?;
        let mut items = rows(
            &conn,
            &format!(
                "{SELECT} {JOINS} WHERE r.enabled=1 AND coalesce(i.included,1)=1 AND i.state IN ('open','draft') AND coalesce(a.type,'User')!='Bot' AND coalesce(a.login,'') NOT LIKE '%[bot]' AND NOT EXISTS(SELECT 1 FROM maintainers m WHERE m.id=i.author_id)"
            ),
            &[],
        )?;
        let now = Utc::now();
        for item in &mut items {
            present(item, now);
        }
        items.sort_by(|a, b| {
            string(&a["engagement"], "at")
                .cmp(string(&b["engagement"], "at"))
                .then(string(a, "id").cmp(string(b, "id")))
        });
        let pending = items
            .iter()
            .filter(|i| i["engagement"]["pending"] == true)
            .cloned()
            .collect::<Vec<_>>();
        let pens = [
            ("day", "Last 24 hours"),
            ("week", "Last week"),
            ("month", "Last month"),
            ("six-months", "Last 6 months"),
            ("cold", "Cold / stale"),
        ]
        .into_iter()
        .map(|(key, label)| {
            let members = items
                .iter()
                .filter(|i| i["engagement"]["pen"] == key)
                .collect::<Vec<_>>();
            json!({"id":key,"label":label,"total":members.len(),"items":members,"hasMore":false})
        })
        .collect::<Vec<_>>();
        let errors = items
            .iter()
            .filter(|i| !i["engagement"]["error"].is_null())
            .count();
        let stale = items
            .iter()
            .filter(|i| i["engagement"]["stale"] == true)
            .count();
        let oldest = items
            .iter()
            .filter_map(|i| i["engagement"]["syncedAt"].as_str())
            .min();
        let maintainers = conn.query_row("SELECT count(*) FROM maintainers", [], |r| {
            r.get::<_, i64>(0)
        })?;
        let repositories = rows(
            &conn,
            "SELECT name FROM repositories WHERE enabled=1 ORDER BY position",
            &[],
        )?
        .iter()
        .map(|r| r["name"].clone())
        .collect::<Vec<_>>();
        let mut repository_sync=rows(&conn,"SELECT r.name AS repo,p.stage,p.active,p.open_page,p.backfill_page,p.last_success,p.error,p.next_retry FROM repositories r LEFT JOIN poll_state p ON p.id=r.id WHERE r.enabled=1 ORDER BY r.position",&[])?;
        for repo in &mut repository_sync {
            let matching:Vec<_>=items.iter().filter(|i|i["repo"]==repo["repo"]).collect();
            let pending=matching.iter().filter(|i|i["engagement"]["pending"]==true||i["engagement"]["stale"]==true).count();
            let discussion_error=matching.iter().find_map(|i|i["engagement"]["error"].as_str());
            let status=if !repo["error"].is_null(){"error"}
                else if repo["stage"].is_null(){"queued"}
                else if repo["stage"]=="opening"||repo["stage"]=="backfill"{if repo["active"]==1{"importing"}else{"waiting"}}
                else if repo["active"]==1{"polling"}
                else if discussion_error.is_some(){"error"}
                else if pending>0{"syncing"}
                else {"ready"};
            repo["status"]=json!(status);if repo["stage"].is_null(){repo["stage"]=json!("");}repo["pending"]=json!(pending);repo["matched"]=json!(matching.len());
            if repo["error"].is_null(){repo["error"]=json!(discussion_error);}
        }
        Ok(
            json!({"vipLogins":rows(&conn,"SELECT coalesce(a.login,c.login) AS login FROM customers c LEFT JOIN authors a ON a.id=c.id",&[])?.iter().filter_map(|p|p["login"].as_str()).collect::<Vec<_>>(),"repositorySync":repository_sync,"total":items.len(),"maintainerCount":maintainers,"repositories":repositories,"pens":pens,"pending":{"total":pending.len(),"items":pending},"sync":{"pending":pending.len(),"errors":errors,"stale":stale,"oldestCheckedAt":oldest,"complete":pending.is_empty()&&errors==0&&stale==0}}),
        )
    }
    pub async fn detail(&self, key: &str) -> Result<Value> {
        let conn = self.db.reader().await?;
        let mut item = rows(
            &conn,
            &format!("{SELECT} {JOINS} WHERE i.id=? AND r.enabled=1"),
            &[json!(key)],
        )?
        .into_iter()
        .next()
        .context("Entry not found")?;
        let mut related = rows(
            &conn,
            "SELECT CASE WHEN rr.enabled=1 THEN t.id ELSE NULL END AS id,e.target_repo AS repo,e.target_number AS number,t.title,coalesce(t.url,e.target_url) AS url,e.kind FROM relationships e LEFT JOIN repositories rr ON rr.name=e.target_repo LEFT JOIN items t ON t.repo_id=rr.id AND t.number=e.target_number WHERE e.source_id=?",
            &[json!(key)],
        )?;
        related.extend(rows(&conn,"SELECT s.id,rr.name AS repo,s.number,s.title,s.url,e.kind FROM relationships e JOIN items s ON s.id=e.source_id JOIN repositories rr ON rr.id=s.repo_id WHERE e.target_repo=? AND e.target_number=? AND rr.enabled=1",&[item["repo"].clone(),item["number"].clone()])?);
        present(&mut item, Utc::now());
        item["related"] = json!(related);
        Ok(item)
    }
    pub async fn advice(&self, key: &str, text: &str, enabled: bool) -> Result<Value> {
        if text.chars().count() > 20000 {
            bail!("Advice is limited to 20,000 characters");
        }
        if self
            .one("SELECT id FROM items WHERE id=?", &[json!(key)])
            .await?
            .is_null()
        {
            bail!("Entry not found");
        }
        let updated = now();
        self.execute(
            "INSERT OR REPLACE INTO advice(id,content,enabled,updated_at) VALUES (?,?,?,?)",
            &[json!(key), json!(text), json!(enabled), json!(updated)],
        )
        .await?;
        Ok(json!({"text":text,"enabled":enabled,"updatedAt":updated}))
    }
}
const SELECT: &str = "SELECT i.*,c.id AS customer_id,c.full_name,c.company,c.vip_company,c.highlight,r.name AS repo,a.login,a.avatar_url,a.type AS author_type,coalesce(d.internal,0) AS internal,coalesce(d.customers,'[]') AS customers,es.synced_at,es.observed_updated_at,es.error AS engagement_error,ev.occurred_at AS event_at,ev.kind AS event_kind,ev.author_id AS event_author_id,ev.url AS event_url,ea.login AS event_login,EXISTS(SELECT 1 FROM maintainers em WHERE em.id=ev.author_id) AS event_maintainer,ad.content AS advice_content,ad.enabled AS advice_enabled,ad.updated_at AS advice_updated_at";
const JOINS: &str = "FROM items i JOIN repositories r ON r.id=i.repo_id LEFT JOIN authors a ON a.id=i.author_id LEFT JOIN customers c ON c.id=i.author_id LEFT JOIN identities d ON d.id=i.author_id LEFT JOIN engagement_sync es ON es.id=i.id LEFT JOIN engagement_events ev ON ev.id=(SELECT ee.id FROM engagement_events ee WHERE ee.item_id=i.id ORDER BY ee.occurred_at DESC,ee.id DESC LIMIT 1) LEFT JOIN authors ea ON ea.id=ev.author_id LEFT JOIN advice ad ON ad.id=i.id";
pub fn human(user: &Value) -> bool {
    user["type"] == "User" && !string(user, "login").ends_with("[bot]")
}
fn save_author(conn: &Connection, user: &Value) -> Result<()> {
    if !user.is_null() {
        upsert(
            conn,
            "authors",
            &json!({"id":id(&user["id"]),"login":user["login"],"avatar_url":user["avatar_url"],"url":user["html_url"],"type":user["type"]}),
        )?;
    }
    Ok(())
}
fn normalize(s: &str) -> String {
    DateTime::parse_from_rfc3339(s)
        .map(|v| {
            v.to_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|_| s.into())
}
fn parsed(value: &Value) -> Value {
    serde_json::from_str(value.as_str().unwrap_or("[]")).unwrap_or(json!([]))
}
pub fn engagement_pen(at: &str, now: DateTime<Utc>) -> Result<&'static str> {
    let time = DateTime::parse_from_rfc3339(at)?.to_utc();
    Ok(if time > now - Duration::hours(24) {
        "day"
    } else if time > now - Duration::days(7) {
        "week"
    } else if time > now.checked_sub_months(Months::new(1)).unwrap() {
        "month"
    } else if time >= now.checked_sub_months(Months::new(6)).unwrap() {
        "six-months"
    } else {
        "cold"
    })
}
fn present(item: &mut Value, now: DateTime<Utc>) {
    item["labels"] = parsed(&item["labels"]);
    item["assignees"] = parsed(&item["assignees"]);
    item["author"] = if item["author_id"].is_null() {
        Value::Null
    } else {
        json!({"vip":!item["customer_id"].is_null(),"fullName":item["full_name"].as_str().unwrap_or(""),"notes":item["company"].as_str().unwrap_or(""),"company":item["vip_company"].as_str().unwrap_or(""),"highlight":item["highlight"].as_str().unwrap_or("gold-star"),"login":item["login"],"internal":item["internal"]==1,"customers":parsed(&item["customers"])})
    };
    let created = item["event_at"].is_null()
        || normalize(string(item, "event_at")) < normalize(string(item, "created_at"));
    let at = normalize(string(
        item,
        if created { "created_at" } else { "event_at" },
    ));
    let pending = item["synced_at"].is_null();
    let stale = !pending
        && (DateTime::parse_from_rfc3339(string(item, "synced_at"))
            .is_ok_and(|t| now - t.to_utc() > Duration::minutes(10))
            || normalize(string(item, "observed_updated_at"))
                < normalize(string(item, "updated_at")));
    item["engagement"] = json!({"at":at,"kind":if created{json!("created")}else{item["event_kind"].clone()},"author":if created{item["author"].clone()}else{json!({"login":item["event_login"],"maintainer":item["event_maintainer"]==1})},"pending":pending,"stale":stale,"error":item["engagement_error"],"syncedAt":item["synced_at"],"pen":if pending{Value::Null}else{json!(engagement_pen(&at,now).ok())}});
    item["advice"] = json!({"text":item["advice_content"].as_str().unwrap_or(""),"enabled":item["advice_enabled"].is_null()||item["advice_enabled"]==1,"updatedAt":item["advice_updated_at"]});
}
pub fn references(body: &str, repo: &str) -> Vec<Value> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut text = String::new();
    let mut code = false;
    let mut image = false;
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = true,
            Event::End(TagEnd::CodeBlock) => code = false,
            Event::Start(Tag::Image { .. }) => image = true,
            Event::End(TagEnd::Image) => image = false,
            Event::Text(s) if !code && !image => {
                text.push(' ');
                text.push_str(&s);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                text.push(' ');
                text.push_str(&dest_url);
            }
            _ => {}
        }
    }
    let urls =
        regex::Regex::new(r"https://github\.com/([\w.-]+/[\w.-]+)/(issues|pull)/(\d+)[^\s<>)]*")
            .unwrap();
    let refs = regex::Regex::new(r"(?:^|[^\w/])(?:([\w.-]+/[\w.-]+))?#(\d+)\b").unwrap();
    let mut found = std::collections::BTreeMap::new();
    for m in urls.captures_iter(&text) {
        if let Ok(number) = m[3].parse::<u64>() {
            let name = m[1].to_lowercase();
            if number > 0 {
                found.insert(format!("{name}#{number}"),json!({"repo":name,"number":number,"url":format!("https://github.com/{}/{}/{}",&m[1],&m[2],number)}));
            }
        }
    }
    let without = urls.replace_all(&text, " ");
    for m in refs.captures_iter(&without) {
        if let Ok(number) = m[2].parse::<u64>() {
            if number > 0 {
                let name = m.get(1).map(|s| s.as_str()).unwrap_or(repo).to_lowercase();
                found.entry(format!("{name}#{number}")).or_insert_with(||json!({"repo":name,"number":number,"url":format!("https://github.com/{name}/issues/{number}")}));
            }
        }
    }
    found.into_values().collect()
}

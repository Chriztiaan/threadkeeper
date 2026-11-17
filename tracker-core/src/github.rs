use crate::store::{human, id, string};
use anyhow::{Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct GitHubError {
    pub status: u16,
    pub retry_at: i64,
    pub message: String,
}
impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for GitHubError {}
fn failure(status: u16, message: &str, retry_at: i64) -> anyhow::Error {
    GitHubError {
        status,
        retry_at,
        message: message.into(),
    }
    .into()
}
pub struct Page {
    pub items: Vec<Value>,
    pub next: Option<u64>,
}
#[async_trait]
pub trait Source: Send {
    async fn repository(&mut self, name: &str) -> Result<Value>;
    async fn issues(
        &mut self,
        name: &str,
        state: &str,
        page: u64,
        since: Option<&str>,
        sort: &str,
    ) -> Result<Page>;
    async fn pull(&mut self, name: &str, number: u64) -> Result<Value>;
    async fn engagement(&mut self, name: &str, number: u64, kind: &str) -> Result<Vec<Value>>;
}
pub struct GitHub {
    client: reqwest::Client,
    base: String,
    delay: Duration,
    last: Option<Instant>,
    blocked_until: i64,
    cooldown_path:Option<std::path::PathBuf>,
    pub auth_failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    limits: u32,
    cache: VecDeque<(String, String, Value, HeaderMap)>,
}
impl GitHub {
    pub fn new(token: String) -> Result<Self> {
        Self::with_base(
            token,
            "https://api.github.com".into(),
            Duration::from_secs(1),
        )
    }
    /// Validate the REST and GraphQL credentials used by the desktop onboarding flow.
    /// Only reads identity data; it never creates or modifies a GitHub resource.
    pub fn restore_cooldown(&mut self,path:std::path::PathBuf){
        if let Some(until)=std::fs::read(&path).ok().and_then(|b|serde_json::from_slice::<i64>(&b).ok()){self.defer_until(until);}
        self.cooldown_path=Some(path);
    }
    fn persist_cooldown(&self){
        if let Some(path)=&self.cooldown_path {let _=(||->std::io::Result<()>{std::fs::create_dir_all(path.parent().unwrap())?;let tmp=path.with_extension("http.tmp");std::fs::write(&tmp,self.blocked_until.to_string())?;std::fs::rename(tmp,path)})();}
    }
    pub fn defer_until(&mut self,until:i64){self.blocked_until=self.blocked_until.max(until);}
    pub fn pause_until(&self)->i64{self.blocked_until}
    pub async fn search_issues(&mut self,query:&str,page:u64)->Result<Page>{
        if self.blocked_until>Utc::now().timestamp_millis(){return Err(failure(429,"GitHub rate limit: polling paused",self.blocked_until));}
        // Search has a separate, lower quota. Pace searches even when other callers are idle.
        if self.base=="https://api.github.com" {tokio::time::sleep(Duration::from_millis(3100)).await;}
        let mut url=reqwest::Url::parse(&format!("{}/search/issues",self.base))?;
        url.query_pairs_mut().append_pair("q",query).append_pair("per_page","100").append_pair("page",&page.to_string()).append_pair("sort","updated").append_pair("order","desc");
        let path=format!("{}?{}",url.path(),url.query().unwrap_or_default());
        let (data,headers)=self.request(&path,None).await?;
        if data["incomplete_results"]==true || data["total_count"].as_u64().unwrap_or(0)>1000 {bail!("GitHub search returned incomplete results or exceeded 1,000 matches; try open-only results or view the full history on GitHub");}
        Ok(Page{items:data["items"].as_array().cloned().ok_or_else(||anyhow::anyhow!("Invalid GitHub search response"))?,next:next(&headers).then_some(page+1)})
    }
    pub async fn pull_details(&mut self,repo:&str,number:u64)->Result<Value>{self.request(&format!("/repos/{repo}/pulls/{number}"),None).await.map(|r|r.0)}
    pub async fn action_since(&mut self,repo:&str,number:u64,login:&str,reasons:&[String])->Result<std::collections::BTreeMap<String,String>>{
        let mut latest=std::collections::BTreeMap::<String,String>::new();let mut page=1;
        loop {
            let (data,headers)=self.request(&format!("/repos/{repo}/issues/{number}/timeline?per_page=100&page={page}"),None).await?;
            for event in data.as_array().ok_or_else(||anyhow::anyhow!("Invalid issue timeline"))? {
                let event_type=event["event"].as_str().unwrap_or_default();
                let who=if event_type=="review_requested" && reasons.iter().any(|r|r=="review") {&event["requested_reviewer"]["login"]}else if event_type=="assigned" && reasons.iter().any(|r|r=="assigned") {&event["assignee"]["login"]}else{continue;};
                if who.as_str().is_some_and(|who|who.eq_ignore_ascii_case(login)) && let Some(at)=event["created_at"].as_str(){let reason=if event_type=="review_requested"{"review"}else{"assigned"};if latest.get(reason).is_none_or(|old|at>old.as_str()){latest.insert(reason.into(),at.into());}}
            }
            if next(&headers){page+=1;}else{break;}
        }
        Ok(latest)
    }
    pub async fn review_summaries(&mut self,repo:&str,number:u64,page:u64)->Result<Page>{
        let (data,headers)=self.request(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=30&page={page}"),None).await?;
        Ok(Page{items:data.as_array().cloned().ok_or_else(||anyhow::anyhow!("Invalid review response"))?,next:next(&headers).then_some(page+1)})
    }
    pub async fn validate_credentials(&mut self) -> Result<()> {
        let (user,_) = self.request("/user",None).await?;
        if user["id"].as_u64().is_none() || user["login"].as_str().is_none() {bail!("GitHub did not return a valid account");}
        let (data,_) = self.request("/graphql",Some(serde_json::json!({"query":"query { viewer { login } }"}))).await?;
        if data["errors"].as_array().is_some_and(|errors|!errors.is_empty()) || data["data"]["viewer"]["login"].as_str().is_none() {
            bail!("This token cannot access GitHub GraphQL. Check its permissions and try again");
        }
        Ok(())
    }
    pub async fn user(&mut self, login: &str) -> Result<Value> {
        let (user,_) = self.request(&format!("/users/{login}"),None).await?;
        if user["type"]!="User" || user["id"].as_u64().is_none() {bail!("Choose an individual GitHub user, rather than an organization or bot");}
        Ok(user)
    }
    /// Ordinary discussion comments for issues and PRs; no reviews or diff threads.
    pub async fn discussion(&mut self, name: &str, number: u64, page: u64) -> Result<Page> {
        let (data, headers) = self.request(&format!("/repos/{name}/issues/{number}/comments?per_page=30&page={page}"), None).await?;
        Ok(Page {
            items: data.as_array().cloned().ok_or_else(|| failure(502, "GitHub returned an invalid discussion", 0))?,
            next: next(&headers).then_some(page + 1),
        })
    }
    pub fn with_base(token: String, base: String, delay: Duration) -> Result<Self> {
        let url = reqwest::Url::parse(&base)?;
        if !(base == "https://api.github.com"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
        {
            bail!("Invalid GitHub API host");
        }
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))?;
        auth.set_sensitive(true);
        headers.insert("authorization", auth);
        headers.insert(
            "accept",
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static("2022-11-28"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent("Threadkeeper/0.1")
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            base,
            delay,
            last: None,
            blocked_until: 0,
            cooldown_path:None,
            auth_failed: Default::default(),
            limits: 0,
            cache: VecDeque::new(),
        })
    }
    async fn request(&mut self, path: &str, body: Option<Value>) -> Result<(Value, HeaderMap)> {
        if self.auth_failed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(failure(
                401,
                "GitHub authentication failed; reconnect GitHub",
                0,
            ));
        }
        if self.blocked_until > Utc::now().timestamp_millis() {
            return Err(failure(
                429,
                "GitHub rate limit: polling paused",
                self.blocked_until,
            ));
        }
        if let Some(last) = self.last {
            if last.elapsed() < self.delay {
                tokio::time::sleep(self.delay - last.elapsed()).await;
            }
        }
        self.last = Some(Instant::now());
        let cached = if body.is_none() {
            self.cache.iter().find(|e| e.0 == path).cloned()
        } else {
            None
        };
        let mut request = if let Some(body) = &body {
            self.client.post(format!("{}{path}", self.base)).json(body)
        } else {
            self.client.get(format!("{}{path}", self.base))
        };
        if let Some((_, etag, _, _)) = &cached {
            request = request.header("if-none-match", etag);
        }
        let response = request
            .send()
            .await
            .map_err(|_| failure(0, "GitHub request interrupted; will retry", 0))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let now = Utc::now().timestamp_millis();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        };
        let remaining = header("x-ratelimit-remaining")
            .parse::<u64>()
            .unwrap_or(u64::MAX);
        let reset = header("x-ratelimit-reset")
            .parse::<i64>()
            .unwrap_or_default()
            * 1000;
        let retry = header("retry-after")
            .parse::<i64>()
            .map(|s| now + s * 1000)
            .unwrap_or_else(|_| {
                chrono::DateTime::parse_from_rfc2822(header("retry-after"))
                    .map(|t| t.timestamp_millis())
                    .unwrap_or_default()
            });
        if status == 401 {
            self.auth_failed.store(true,std::sync::atomic::Ordering::Relaxed);
            return Err(failure(
                status,
                "GitHub authentication failed; reconnect GitHub",
                0,
            ));
        }
        let data = if status == 304 {
            Value::Null
        } else {
            response.json::<Value>().await.unwrap_or(Value::Null)
        };
        let message = string(&data, "message").to_lowercase();
        let rate = status == 429
            || (status == 403
                && (remaining == 0
                    || retry > 0
                    || message.contains("rate limit")
                    || message.contains("abuse")))
            || data["errors"]
                .as_array()
                .is_some_and(|e| e.iter().any(|e| e["type"] == "RATE_LIMITED"));
        if rate {
            self.limits += 1;
            self.blocked_until = (now
                + (60000i64.saturating_mul(1i64 << self.limits.min(6).saturating_sub(1)))
                    .min(1800000))
            .max(retry)
            .max(if remaining == 0 { reset } else { 0 })
                + 1000;
            self.persist_cooldown();
            return Err(failure(
                429,
                "GitHub rate limit: polling paused",
                self.blocked_until,
            ));
        }
        if !(200..300).contains(&status) && status != 304 {
            return Err(failure(
                status,
                if status == 404 {
                    "Repository or entry unavailable (not found or no access)"
                } else if status == 403 {
                    "GitHub denied access; check token permissions"
                } else {
                    "GitHub request failed"
                },
                0,
            ));
        }
        if data["errors"].as_array().is_some_and(|e| !e.is_empty()) {
            return Err(failure(502, "GitHub returned incomplete PR data", 0));
        }
        self.limits = 0;
        if remaining < 100 && reset > now {
            self.blocked_until = reset + 1000;
        }
        if data["data"]["rateLimit"]["remaining"]
            .as_u64()
            .is_some_and(|r| r < 100)
        {
            if let Ok(reset) =
                chrono::DateTime::parse_from_rfc3339(string(&data["data"]["rateLimit"], "resetAt"))
            {
                self.blocked_until = self.blocked_until.max(reset.timestamp_millis() + 1000);
            }
        }
        if status == 304 {
            if let Some((_, _, data, headers)) = cached {
                return Ok((data, headers));
            }
            bail!("GitHub returned an uncached conditional response");
        }
        if body.is_none() {
            if let Some(etag) = headers.get("etag").and_then(|v| v.to_str().ok()) {
                self.cache.retain(|entry| entry.0 != path);
                if self.cache.len() >= 128 {
                    self.cache.pop_front();
                }
                self.cache
                    .push_back((path.into(), etag.into(), data.clone(), headers.clone()));
            }
        }
        Ok((data, headers))
    }
}
fn next(headers: &HeaderMap) -> bool {
    headers
        .get("link")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("rel=\"next\""))
}
#[async_trait]
impl Source for GitHub {
    async fn repository(&mut self, name: &str) -> Result<Value> {
        let (data, _) = self.request(&format!("/repos/{name}"), None).await?;
        if !data["id"].is_number() || !data["full_name"].is_string() {
            bail!("Invalid repository metadata");
        }
        Ok(data)
    }
    async fn issues(
        &mut self,
        name: &str,
        state: &str,
        page: u64,
        since: Option<&str>,
        sort: &str,
    ) -> Result<Page> {
        let mut url = reqwest::Url::parse(&format!("{}/repos/{name}/issues", self.base))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("state", state)
                .append_pair("sort", sort)
                .append_pair("direction", "asc")
                .append_pair("per_page", "100")
                .append_pair("page", &page.to_string());
            if let Some(since) = since {
                q.append_pair("since", since);
            }
        }
        let path = format!("{}?{}", url.path(), url.query().unwrap());
        let (data, headers) = self.request(&path, None).await?;
        Ok(Page {
            items: data
                .as_array()
                .cloned()
                .ok_or_else(|| failure(502, "GitHub returned an invalid item list", 0))?,
            next: next(&headers).then_some(page + 1),
        })
    }
    async fn pull(&mut self, name: &str, number: u64) -> Result<Value> {
        let (owner, repo) = name
            .split_once('/')
            .ok_or_else(|| failure(0, "Invalid repository name", 0))?;
        let mut after = Value::Null;
        let mut result = Value::Null;
        loop {
            let(data,_)=self.request("/graphql",Some(json!({"query":"query($owner:String!,$repo:String!,$number:Int!,$after:String){repository(owner:$owner,name:$repo){pullRequest(number:$number){isDraft mergedAt reviewRequests(first:100){nodes{requestedReviewer{... on User{login}}} pageInfo{hasNextPage}} closingIssuesReferences(first:100,after:$after){nodes{url number repository{nameWithOwner}} pageInfo{hasNextPage endCursor}}}} rateLimit{remaining resetAt}}","variables":{"owner":owner,"repo":repo,"number":number,"after":after}}))).await?;
            let pr = &data["data"]["repository"]["pullRequest"];
            if pr.is_null() {
                return Err(failure(404, "Pull request unavailable", 0));
            }
            if result.is_null() {
                result = json!({"isDraft":pr["isDraft"],"mergedAt":pr["mergedAt"],"closing":[]});
                if pr["reviewRequests"]["pageInfo"]["hasNextPage"] == false {
                    if let Some(nodes)=pr["reviewRequests"]["nodes"].as_array() {
                        result["requested_reviewers"]=json!(nodes.iter().filter_map(|n|n["requestedReviewer"]["login"].as_str().map(|login|json!({"login":login}))).collect::<Vec<_>>());
                    }
                }
            }
            let nodes = pr["closingIssuesReferences"]["nodes"]
                .as_array()
                .ok_or_else(|| failure(502, "Incomplete closing issue references", 0))?;
            result["closing"]
                .as_array_mut()
                .unwrap()
                .extend(nodes.iter().cloned());
            let info = &pr["closingIssuesReferences"]["pageInfo"];
            if info["hasNextPage"] != true {
                break;
            }
            if !info["endCursor"].is_string() || info["endCursor"] == after {
                bail!("Invalid GraphQL pagination cursor");
            }
            after = info["endCursor"].clone();
        }
        Ok(result)
    }
    async fn engagement(&mut self, name: &str, number: u64, kind: &str) -> Result<Vec<Value>> {
        let mut sources = vec![(
            format!("/repos/{name}/issues/{number}/comments"),
            "comment",
            "created_at",
        )];
        if kind == "pr" {
            sources.extend([
                (
                    format!("/repos/{name}/pulls/{number}/reviews"),
                    "review",
                    "submitted_at",
                ),
                (
                    format!("/repos/{name}/pulls/{number}/comments"),
                    "review-comment",
                    "created_at",
                ),
            ]);
        }
        let mut result = vec![];
        for (path, kind, date) in sources {
            let mut page = 1;
            loop {
                let (data, headers) = self
                    .request(&format!("{path}?per_page=100&page={page}"), None)
                    .await?;
                let events = data
                    .as_array()
                    .ok_or_else(|| failure(502, "GitHub returned incomplete discussion data", 0))?;
                for event in events {
                    if human(&event["user"])
                        && event["state"] != "PENDING"
                        && chrono::DateTime::parse_from_rfc3339(string(event, date)).is_ok()
                    {
                        result.push(json!({"id":id(&event["id"]),"kind":kind,"at":event[date],"user":event["user"],"url":event["html_url"]}));
                    }
                }
                if !next(&headers) {
                    break;
                }
                page += 1;
            }
        }
        Ok(result)
    }
}

/// All consumers share one paced client, including repository-access validation.
#[derive(Clone)]
pub struct SharedGitHub(pub std::sync::Arc<tokio::sync::Mutex<GitHub>>);
impl SharedGitHub {
    pub async fn replace(&self,mut client:GitHub) {
        let mut current=self.0.lock().await;
        client.auth_failed=current.auth_failed.clone();
        client.cooldown_path=current.cooldown_path.clone();client.defer_until(current.blocked_until);
        *current=client;
        current.auth_failed.store(false,std::sync::atomic::Ordering::Relaxed);
    }
    pub fn new(github: GitHub) -> Self {
        Self(std::sync::Arc::new(tokio::sync::Mutex::new(github)))
    }
}
#[async_trait]
impl Source for SharedGitHub {
    async fn repository(&mut self, name: &str) -> Result<Value> {
        self.0.lock().await.repository(name).await
    }
    async fn issues(
        &mut self,
        name: &str,
        state: &str,
        page: u64,
        since: Option<&str>,
        sort: &str,
    ) -> Result<Page> {
        self.0
            .lock()
            .await
            .issues(name, state, page, since, sort)
            .await
    }
    async fn pull(&mut self, name: &str, number: u64) -> Result<Value> {
        self.0.lock().await.pull(name, number).await
    }
    async fn engagement(&mut self, name: &str, number: u64, kind: &str) -> Result<Vec<Value>> {
        self.0.lock().await.engagement(name, number, kind).await
    }
}

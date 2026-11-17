use crate::{State, data};
use rune::{Latest, TextEdit};
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preference {
    login: String,
}
pub const QUEUES: [(&str, &str); 4] = [
    ("review", "Review requested"),
    ("assigned", "Assigned"),
    ("created", "Created"),
    ("participating", "Participating"),
];
pub fn title(queue: &str) -> &str {
    QUEUES
        .iter()
        .find(|q| q.0 == queue)
        .map(|q| q.1)
        .unwrap_or("Developer work")
}
pub fn reason(reason: &str) -> &str {
    match reason {
        "review" => "Review requested",
        "assigned" => "Assigned",
        "author" => "Author",
        "mentioned" => "Mentioned",
        "commented" => "Commented",
        _ => reason,
    }
}
pub fn matches(item: &data::Item, queue: &str, closed: bool) -> bool {
    if !closed && matches!(item.state.as_str(), "closed" | "merged") {
        return false;
    }
    item.developer_reasons.iter().any(|r| match queue {
        "created" => r == "author",
        "participating" => matches!(r.as_str(), "author" | "mentioned" | "commented"),
        _ => r == queue,
    })
}
pub fn sort_at<'a>(item: &'a data::Item, queue: &str) -> &'a str {
    if matches!(queue, "review" | "assigned") {
        item.action_times
            .get(queue)
            .map(String::as_str)
            .unwrap_or(&item.created_at)
    } else {
        &item.updated_at
    }
}
pub struct Developer {
    pub active: bool,
    pub login: String,
    pub profiles: Vec<String>,
    pub picker: bool,
    pub queue: String,
    pub closed: bool,
    pub items: Vec<data::Item>,
    pub loading: bool,
    pub error: Option<String>,
    pub checked: Option<i64>,
    pub retry_at: Option<i64>,
    pub paused: bool,
    pub loaded_queues: Vec<String>,
    pub next: f64,
    pub results: Latest<(String, bool, String, Result<data::DeveloperResult, String>)>,
    pub profiles_result:
        Latest<Result<tracker_core::maintainer_config::MaintainerSettings, String>>,
    pub followup_newest: bool,
    pub followup_kind: String,
    pub followup_search: String,
}
impl Developer {
    pub fn saved_login() -> Option<String> {
        load_preference(&tracker_core::config::data_dir().join("developer-profile.json"))
    }
    pub fn new(preview: bool) -> Self {
        let login = if preview {
            "Chriztiaan".into()
        } else {
            std::fs::read(tracker_core::config::data_dir().join("developer-profile.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Preference>(&b).ok())
                .map(|p| p.login)
                .filter(|s| !s.is_empty())
                .unwrap_or("Chriztiaan".into())
        };
        Self {
            active: false,
            login,
            profiles: vec![],
            picker: false,
            queue: "review".into(),
            closed: false,
            items: vec![],
            loading: false,
            error: None,
            checked: None,
            retry_at: None,
            paused: false,
            loaded_queues: vec![],
            next: 0.0,
            results: Latest::default(),
            profiles_result: Latest::default(),
            followup_newest: false,
            followup_kind: String::new(),
            followup_search: String::new(),
        }
    }
    fn save(&self) -> Result<(), String> {
        save_preference(
            &tracker_core::config::data_dir().join("developer-profile.json"),
            &self.login,
        )
    }
}
impl State {
    pub(crate) fn followup_matches(&self, item: &data::Item) -> bool {
        let kind = if self.dev.active {
            &self.dev.followup_kind
        } else {
            &self.kind
        };
        let search = if self.dev.active {
            &self.dev.followup_search
        } else {
            &self.search.text
        };
        self.repo_scope.includes(&item.repo)
            && (kind.is_empty() || *kind == item.kind)
            && (search.is_empty()
                || format!(
                    "{} {} {} #{}",
                    item.title,
                    item.repo,
                    item.author.as_ref().map(|a| a.login.as_str()).unwrap_or(""),
                    item.number
                )
                .to_lowercase()
                .contains(&search.to_lowercase()))
    }

    pub(crate) fn developer_queue(&mut self, queue: &str) {
        if !self.preview
            && let Some(login) = Developer::saved_login()
        {
            if login != self.dev.login {
                self.choose_developer(login);
            }
        }
        self.remember_note();
        if !self.dev.active {
            self.dev.followup_newest = self.newest;
            self.dev.followup_kind = self.kind.clone();
            self.dev.followup_search = self.search.text.clone();
            self.kind.clear();
            self.search = TextEdit::new("");
        }
        self.dev.active = true;
        self.dev.queue = queue.into();
        self.newest = matches!(queue, "created" | "participating");
        self.repo_picker_open = false;
        self.items = self.dev.items.clone();
        self.refilter();
        if self.dev.profiles.is_empty() && !self.preview {
            self.dev
                .profiles_result
                .request((), |_| data::maintainer_settings());
        }
        if self
            .dev
            .profiles
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&self.dev.login))
        {
            self.load_developer(false);
        }
    }
    pub(crate) fn followup(&mut self, pen: &str) {
        self.remember_note();
        if self.dev.active {
            self.newest = self.dev.followup_newest;
            self.kind = self.dev.followup_kind.clone();
            self.search = TextEdit::new(&self.dev.followup_search);
        }
        self.dev.active = false;
        self.dev.picker = false;
        self.pen = pen.into();
        self.items = self.data.items();
        self.refilter();
    }
    pub(crate) fn choose_developer(&mut self, login: String) {
        self.remember_note();
        self.dev.login = login;
        self.dev.picker = false;
        self.dev.items.clear();
        self.dev.loading = false;
        self.dev.checked = None;
        self.dev.loaded_queues.clear();
        self.dev.retry_at = None;
        self.dev.paused = false;
        if !self.preview {
            self.dev.error = self.dev.save().err();
        }
        if self.dev.active {
            self.items.clear();
            self.refilter();
            self.load_developer(false);
        } else {self.load_developer_cache();}
    }
    pub(crate) fn load_developer(&mut self, force: bool) {
        self.request_developer(force,self.dev.queue.clone());
    }
    pub(crate) fn load_developer_cache(&mut self) {
        if !self.settings_window {self.request_developer(false,"cached".into());}
    }
    fn request_developer(&mut self, force:bool, queue:String) {
        if self.preview || self.auth_blocked() || self.dev.loading || self.dev.login.is_empty() {
            return;
        }
        self.dev.loading = true;
        self.dev.error = None;
        self.dev.results.request(
            (
                self.dev.login.clone(),
                self.dev.closed,
                force,
                queue,
            ),
            |(login, closed, force, queue)| {
                let result = data::developer_work(login.clone(), closed, force, queue.clone());
                (login, closed, queue, result)
            },
        );
    }
    pub(crate) fn poll_developer(&mut self, now: f64) -> bool {
        let mut changed = false;
        if !self.preview
            && !self.settings_window
            && let Some(login) = Developer::saved_login()
        {
            if login != self.dev.login {
                self.choose_developer(login);
                changed = true;
            }
        }
        if let Some(result) = self.dev.profiles_result.take() {
            changed = true;
            match result {
                Ok(settings) => {
                    self.dev.profiles = settings
                        .maintainers
                        .iter()
                        .map(|p| p.login.clone())
                        .collect();
                    if !self
                        .dev
                        .profiles
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(&self.dev.login))
                    {
                        self.dev.login = self.dev.profiles.first().cloned().unwrap_or_default();
                        self.dev.items.clear();
                        if !self.preview {
                            self.dev.error = self.dev.save().err();
                        }
                        if self.dev.active {
                            self.items.clear();
                            self.refilter();
                        }
                    }
                    if !self.dev.login.is_empty() {
                        if self.dev.active {self.load_developer(false);} else {self.load_developer_cache();}
                    }
                }
                Err(e) => self.dev.error = Some(e),
            }
        }
        if let Some((login, closed, queue, result)) = self.dev.results.take() {
            if login.eq_ignore_ascii_case(&self.dev.login) && closed == self.dev.closed {
                changed = true;
                self.dev.loading = false;
                self.dev.next = now + 900.0;
                match result {
                    Ok(result) => {
                        self.dev.error = result.warning;
                        self.dev.checked = if queue == self.dev.queue {
                            result.checked
                        } else {
                            None
                        };
                        self.dev.retry_at = result.retry_at;
                        self.dev.paused = result.paused;
                        self.dev.loaded_queues = result.loaded_queues;
                        if let Some(retry) = self.dev.retry_at {
                            self.dev.next = now
                                + ((retry - chrono::Utc::now().timestamp_millis()).max(1000)
                                    as f64
                                    / 1000.0);
                        }
                        let different = self.dev.items != result.items;
                        self.dev.items = result.items;
                        if self.dev.active && different {
                            self.remember_note();
                            self.items = self.dev.items.clone();
                            self.rebuild();
                            self.restore_note();
                            self.load_detail();
                        }
                    }
                    Err(e) => self.dev.error = Some(e),
                }
                if queue != self.dev.queue && self.dev.active {
                    self.load_developer(false);
                }
            }
        }
        if !self.preview && !self.settings_window && now >= self.dev.next && !self.dev.loading {
            self.dev.next = now + 900.0;
            self.dev
                .profiles_result
                .request((), |_| data::maintainer_settings());
        }
        changed
    }
}

pub(crate) fn preview(s: &mut State) {
    s.dev.loaded_queues = QUEUES.iter().map(|q| q.0.into()).collect();
    s.dev.profiles = vec!["Chriztiaan".into(), "alice".into()];
    s.dev.login = "Chriztiaan".into();
    s.dev.items=serde_json::from_value(serde_json::json!([
        {"id":"demo-review","repo":"powersync-ja/powersync-js","number":312,"type":"pr","title":"Improve reconnect handling for offline clients","state":"open","author":{"login":"alice","customer":true,"highlight":"gold-star"},"body":"The new reconnect flow preserves local data and resumes syncing after credentials are replaced.","comments":2,"developerReasons":["review","assigned","mentioned"],"actionTimes":{"review":"2026-11-12T09:00:00Z","assigned":"2026-11-10T09:00:00Z"},"created_at":"2026-11-10T09:00:00Z","updated_at":"2026-11-16T09:00:00Z","url":"https://github.com/powersync-ja/powersync-js"},
        {"id":"demo-own","repo":"tanstack/db","number":2366,"type":"pr","title":"Add PowerSync integration example","state":"open","author":{"login":"Chriztiaan"},"developerReasons":["author","commented","review"],"actionTimes":{"review":"2026-11-14T09:00:00Z"},"created_at":"2026-11-11T09:00:00Z","updated_at":"2026-11-15T09:00:00Z"}
    ])).unwrap();
    s.developer_queue("review");
    s.select("demo-review".into());
    s.show_advice = false;
    s.dev.checked = Some(chrono::Utc::now().timestamp());
    s.discussions.insert(
        "demo-review".into(),
        crate::DiscussionState {
            loaded: true,
            comments: vec![
                data::Comment {
                    id: 1,
                    kind: "comment".into(),
                    author: "alice".into(),
                    body: "Ready for another look. The local cache survives a failed request."
                        .into(),
                    created_at: "2026-11-14T09:00:00Z".into(),
                    updated_at: "2026-11-14T09:00:00Z".into(),
                    ..Default::default()
                },
                data::Comment {
                    id: 2,
                    kind: "review".into(),
                    review_state: "CHANGES_REQUESTED".into(),
                    author: "Chriztiaan".into(),
                    body: "Please preserve the current selection while the refresh completes."
                        .into(),
                    created_at: "2026-11-15T09:00:00Z".into(),
                    updated_at: "2026-11-15T09:00:00Z".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    );
}

pub(crate) fn load_preference(path: &std::path::Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Preference>(&b).ok())
        .map(|p| p.login)
        .filter(|v| !v.is_empty())
}
pub(crate) fn save_preference(path: &std::path::Path, login: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.new");
    std::fs::write(
        &tmp,
        serde_json::to_vec(&Preference {
            login: login.into(),
        })
        .unwrap(),
    )
    .and_then(|_| std::fs::rename(tmp, path))
    .map_err(|e| e.to_string())
}

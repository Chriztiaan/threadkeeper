mod showcase;
mod data;
mod auth;
mod developer;
mod scope;
mod preferences;
mod workspace;
use scope::RepoScope;
mod ui;
#[cfg(test)] mod tests;

use data::{Advice, Item, Snapshot};
use rune::{App, AppSpec, Latest, TextEdit, Tx, Waker, Key, KeyEvent, Titlebar, WindowOptions};
use std::{collections::HashMap, time::Duration};

enum Connection { Cached, Online, Offline(String) }
#[derive(Clone)]
enum NoteState { Clean, Dirty, Saving, Saved, Failed(String) }
#[derive(Default)]
struct CommentDisplay { expanded:bool, collapsed:bool }
#[derive(Default)]
struct DiscussionState {
    version: String, loaded: bool, comments: Vec<data::Comment>, next: Option<u64>, loading: bool, error: Option<String>, warning: Option<String>,
}
static MAIN_APP: std::sync::OnceLock<rune::AppHandle<State>> = std::sync::OnceLock::new();
static SETTINGS_WINDOW: std::sync::OnceLock<(rune::AppHandle<State>,rune::WindowHandle)> = std::sync::OnceLock::new();
struct State {
    workspace:workspace::Import,
    dev:developer::Developer,
    auth:auth::Setup, runtime_waker:Option<Waker>,
    demo_saved:Option<showcase::SavedSettings>, demo:bool, settings_close_confirm:bool, settings_window:bool, settings_tab:String, maint_prefs:preferences::MaintainerPreferences,
    maint_settings:Latest<Result<tracker_core::maintainer_config::MaintainerSettings,String>>,
    maint_saved:Latest<Result<tracker_core::maintainer_config::MaintainerSettings,String>>,
    maint_imports:Latest<Result<Vec<tracker_core::config::Maintainer>,String>>,
    cust_prefs:preferences::CustomerPreferences,
    vip_profile_target:Option<String>,
    cust_settings:Latest<Result<tracker_core::customer_config::CustomerSettings,String>>,
    cust_saved:Latest<Result<tracker_core::customer_config::CustomerSettings,String>>,
    cust_imports:Latest<Result<Vec<tracker_core::customer_config::Customer>,String>>,
    preview_id: Option<String>, detail_error: Option<String>,
    discussions: HashMap<String,DiscussionState>,
    comment_display: HashMap<String,CommentDisplay>,
    discussion_results: Latest<(String,u64,String,Result<data::Discussion,String>)>,
    repo_prefs:preferences::RepositoryPreferences,
    repo_imports:Latest<Result<Vec<tracker_core::config::Repository>,String>>,
    repo_settings:Latest<Result<tracker_core::repository_config::RepositorySettings,String>>,
    repo_settings_saved:Latest<Result<tracker_core::repository_config::RepositorySettings,String>>,
    data: Snapshot, items: Vec<Item>, visible: Vec<usize>, selected: String,
    pen: String, repo_scope:RepoScope, repo_picker_open:bool, repo_search:TextEdit, scope_error:Option<String>, kind: String, newest: bool,
    search: TextEdit, note: TextEdit, note_enabled: bool, note_state: NoteState,
    drafts: HashMap<String, (String, bool)>, detail: Option<Item>,
    connection: Connection, appearance: Option<bool>, show_advice: bool,
    snapshots: Latest<Result<Snapshot, String>>, details: Latest<(String, Result<Item, String>)>,
    github_refresh: Latest<Result<data::RefreshStatus,String>>, refreshing: bool,
    refresh_status: Option<data::RefreshStatus>, refresh_at: Option<f64>, refresh_dismiss_at: Option<f64>,
    saves: Latest<(String, Result<Advice, String>)>, next_refresh: f64, preview: bool, revision:u64,
}
impl State {
    fn age(&self,at:&str)->String {if self.demo{data::age_at(at,chrono::DateTime::parse_from_rfc3339("2026-11-17T12:00:00Z").unwrap().with_timezone(&chrono::Utc))}else{data::age(at)}}
    fn init() -> Self {
        let preview=auth::demo_requested() || std::env::args().any(|a| ["--snap", "--tree", "--script", "--list-scenes", "--out", "--video"].contains(&a.as_str()));
        let data = if preview {Snapshot::default()}else{std::fs::read_to_string(data::cache_path("desktop-followup.json")).ok().and_then(|s| serde_json::from_str::<Snapshot>(&s).ok()).unwrap_or_default()};
        let items = data.items();
        let detail = if preview {None}else{std::fs::read_to_string(data::cache_path("desktop-detail.json")).ok().and_then(|s| serde_json::from_str::<Item>(&s).ok())};
        let repo_scope=if preview {RepoScope::default()}else{RepoScope::load(&tracker_core::config::data_dir().join("repository-scope.json"))};
        let mut state = Self {
            workspace:Default::default(),
            dev:developer::Developer::new(preview),
            auth:auth::Setup{ready:preview,..Default::default()},runtime_waker:None,
            demo_saved:None,demo:false,settings_close_confirm:false,settings_window:false,settings_tab:"repositories".into(),maint_prefs:Default::default(),maint_settings:Latest::default(),maint_saved:Latest::default(),maint_imports:Latest::default(),
            cust_prefs:Default::default(),vip_profile_target:None,cust_settings:Latest::default(),cust_saved:Latest::default(),cust_imports:Latest::default(),
            preview_id:None,detail_error:None,discussions:HashMap::new(),comment_display:HashMap::new(),discussion_results:Latest::default(),
            repo_prefs:Default::default(),repo_imports:Latest::default(),repo_settings:Latest::default(),repo_settings_saved:Latest::default(),
            data, items, visible: vec![], selected: String::new(), pen: String::new(), repo_scope, repo_picker_open:false, repo_search:TextEdit::new(""),scope_error:None, kind: String::new(), newest: false,
            search: TextEdit::new(""), note: TextEdit::new(""), note_enabled: true, note_state: NoteState::Clean,
            drafts: HashMap::new(), detail, connection: Connection::Cached, appearance: None, show_advice: true,
            github_refresh: Latest::default(), refreshing: false, refresh_status: None, refresh_at: None, refresh_dismiss_at: None,
            snapshots: Latest::default(), details: Latest::default(), saves: Latest::default(), next_refresh: 60.0, revision:0,
            preview,
        };
        state.rebuild(); state.restore_note();
        if auth::demo_requested(){state.auth=auth::Setup::default();if std::env::var("GITHUB_TOKEN").is_ok_and(|v|v.trim()=="mock_token") && std::env::var("THREADKEEPER_DEMO").as_deref()!=Ok("1"){state.connected("mock_token".into());}}
        state
    }
    fn settings_init()->Self{let mut s=Self::init();s.settings_window=true;s.repo_prefs.open=true;if !s.auth_blocked(){s.reload_repository_preferences();}s}
    fn auth_blocked(&self)->bool{!self.auth.ready&&!data::authorized()}
    fn connect_github(&mut self){
        if self.auth.token.text.trim()=="mock_token" {self.connected("mock_token".into());}else if auth::demo_requested(){self.auth.error=Some("Use mock_token in this isolated demo.".into());}else{self.auth.connect();}
    }
    fn connected(&mut self,token:String){
        if token=="mock_token" {
            if data::authorized(){data::shutdown();}
            self.preview=true;self.demo=true;
            self.snapshots.cancel();self.details.cancel();self.dev.results.cancel();self.discussion_results.cancel();self.github_refresh.cancel();self.cust_settings.cancel();self.auth.discovered.cancel();
            self.auth=auth::Setup{ready:true,started:true,..Default::default()};
            self.drafts.clear();self.discussions.clear();self.detail=None;self.repo_prefs=Default::default();self.maint_prefs=Default::default();self.cust_prefs=Default::default();
            showcase::demo(self);return;
        }
        self.auth.checking=false;self.auth.saving=false;self.auth.error=None;
        self.auth.token=TextEdit::new("").secure();self.auth.ready=true;self.auth.disconnected=false;self.auth.reconnect_open=false;
        if self.preview{return;}
        data::authorize(token);
        if !self.settings_window{self.reload_customers();}
        if let Some(wake)=self.runtime_waker.clone(){data::start(move||wake());}
        self.refresh();
    }
    fn import_repository_file(&mut self,tx:&mut Tx){
        if self.repo_prefs.saving||self.repo_prefs.loading||self.repo_prefs.importing{return;}
        let options=rune::FileDialogOptions{title:"Import repository configuration".into(),..Default::default()}.filter("Repository configuration",&["json"]);
        tx.open_file(options,|s:&mut State,result,_|match result{
            Ok(Some(path))=>{s.repo_prefs.importing=true;s.repo_prefs.error=None;s.repo_prefs.notice=None;s.repo_imports.request(path,preferences::read_import);},
            Ok(None)=>{},
            Err(error)=>s.repo_prefs.error=Some(error.to_string()),
        });
    }
    fn open_repository_preferences(&mut self) {
        if self.auth_blocked(){return;}
        self.repo_picker_open=false;
        if !self.settings_window && !self.preview && let Some((app,window))=SETTINGS_WINDOW.get(){
            let appearance=self.appearance;
            app.update(move|s,_|{s.appearance=appearance;if !s.repo_prefs.open||(!s.repo_prefs.is_dirty()&&!s.maint_prefs.dirty&&!s.cust_prefs.is_dirty()&&!s.settings_busy()){s.maint_prefs.loaded=false;s.cust_prefs.loaded=false;s.repo_prefs.open=true;s.reload_repository_preferences();if s.settings_tab=="maintainers"{s.reload_maintainers();}else if s.settings_tab=="customers"{s.reload_customers();}else if s.settings_tab=="workspace"{s.reload_maintainers();s.reload_customers();}}});
            window.open();return;
        }
        if !self.demo{self.maint_prefs.loaded=false;self.cust_prefs.loaded=false;}
        self.repo_picker_open=false;self.repo_prefs.open=true;
        self.reload_repository_preferences();
        if self.settings_tab=="workspace"{self.reload_maintainers();self.reload_customers();}
    }
    fn reload_repository_preferences(&mut self){
        if self.demo{showcase::repositories(self);return;}
        if self.repo_prefs.saving||self.repo_prefs.importing{return;}
        self.repo_prefs.loading=true;self.repo_prefs.error=None;
        if self.preview{self.repo_prefs.load(tracker_core::repository_config::RepositorySettings{repositories:self.repositories().into_iter().map(|name|tracker_core::config::Repository{name,include_terms: vec![],enabled:true}).collect(),path:"repositories.json".into(),..Default::default()});}
        else{self.repo_settings.request((),|_|data::repository_settings());}
    }
    fn save_repository_preferences(&mut self){
        if self.repo_prefs.saving||self.repo_prefs.loading||self.repo_prefs.importing{return;}
        self.repo_prefs.saving=true;self.repo_prefs.error=None;
        if self.preview{if let Some(saved)=self.demo_saved.as_mut(){saved.repositories=self.repo_prefs.repositories();}let settings=tracker_core::repository_config::RepositorySettings{repositories:self.repo_prefs.repositories(),path:self.repo_prefs.path.clone(),..Default::default()};self.repo_prefs.load(settings);if !self.settings_window&&!self.demo{self.repo_prefs.open=false;}return;}
        self.repo_settings_saved.request((self.repo_prefs.repositories(),self.repo_prefs.revision.clone()),|(repos,revision)|data::save_repositories(repos,revision));
    }
    fn settings_dirty(&self)->bool {self.workspace.draft.is_some()||self.repo_prefs.is_dirty()||self.maint_prefs.dirty||self.cust_prefs.is_dirty()}
    fn discard_settings(&mut self) {
        if self.settings_busy(){return;}
        self.workspace.result.cancel();self.workspace.draft=None;self.workspace.error=None;self.workspace.notice=None;self.settings_close_confirm=false;self.repo_prefs.open=false;
        self.repo_prefs=Default::default();self.maint_prefs=Default::default();self.cust_prefs=Default::default();
    }
    fn request_settings_close(&mut self) {
        if self.settings_busy(){return;}
        if self.settings_dirty(){self.settings_close_confirm=true;}else{self.repo_prefs.open=false;}
    }
    fn settings_busy(&self)->bool{self.workspace.reading||self.workspace.applying||self.repo_prefs.saving||self.repo_prefs.importing||self.maint_prefs.saving||self.maint_prefs.importing||self.cust_prefs.saving||self.cust_prefs.importing}
    fn settings_page(&mut self,tab:&str){
        if self.settings_busy(){return;}
        self.dev.picker=false;self.settings_tab=tab.into();if tab=="workspace"{if !self.maint_prefs.loaded&&!self.maint_prefs.loading{self.reload_maintainers();}if !self.cust_prefs.loaded&&!self.cust_prefs.loading{self.reload_customers();}}if tab=="developer" {if self.preview{if !self.demo{self.dev.profiles=vec!["Chriztiaan".into(),"example-maintainer".into()];}}else{self.dev.profiles_result.request((),|_|data::maintainer_settings());}}if tab=="maintainers"&&!self.maint_prefs.loaded&&!self.maint_prefs.loading{self.reload_maintainers();}else if tab=="customers"&&!self.cust_prefs.loaded&&!self.cust_prefs.loading{self.reload_customers();}
    }
    fn reload_maintainers(&mut self){
        if self.demo{showcase::maintainers(self);return;}
        if self.maint_prefs.saving||self.maint_prefs.importing{return;}
        self.maint_prefs.loading=true;self.maint_prefs.error=None;
        if self.preview{self.maint_prefs.load(tracker_core::maintainer_config::MaintainerSettings{maintainers:serde_json::from_str::<tracker_core::maintainer_config::MaintainerFile>(include_str!("../../maintainers.json")).map(|f|f.maintainers).unwrap_or_default(),path:"maintainers.json".into(),..Default::default()});}
        else{self.maint_settings.request((),|_|data::maintainer_settings());}
    }
    fn save_maintainers(&mut self){
        if self.maint_prefs.busy(){return;}self.maint_prefs.saving=true;self.maint_prefs.error=None;
        if self.preview{if let Some(saved)=self.demo_saved.as_mut(){saved.maintainers=self.maint_prefs.rows.clone();}self.maint_prefs.saving=false;self.maint_prefs.dirty=false;return;}
        self.maint_saved.request((self.maint_prefs.rows.clone(),self.maint_prefs.revision.clone()),|(people,revision)|data::save_maintainers(people,revision));
    }
    fn import_maintainer_file(&mut self,tx:&mut Tx){
        if self.maint_prefs.busy(){return;}
        tx.open_file(rune::FileDialogOptions{title:"Import maintainer roster".into(),..Default::default()}.filter("Maintainer roster",&["json"]),|s:&mut State,result,_|match result{
            Ok(Some(path))=>{s.maint_prefs.importing=true;s.maint_prefs.error=None;s.maint_imports.request(path,preferences::read_maintainer_import);},
            Ok(None)=>{},Err(error)=>s.maint_prefs.error=Some(error.to_string()),
        });
    }
    fn open_vip_profile(&mut self, login:String, tx:&mut Tx) {
        let Ok(login)=tracker_core::customer_config::normalize_login(&login) else{return;};
        if !self.settings_window && !self.preview && let Some((app,window))=SETTINGS_WINDOW.get() {
            let appearance=self.appearance;
            app.update(move|s,tx|{s.appearance=appearance;s.prepare_vip_profile(login,tx);});
            window.open();
        } else {self.prepare_vip_profile(login,tx);}
    }
    fn prepare_vip_profile(&mut self,login:String,tx:&mut Tx) {
        self.repo_picker_open=false;self.repo_prefs.open=true;self.settings_tab="customers".into();
        self.vip_profile_target=Some(login);
        if self.cust_prefs.loaded && (self.cust_prefs.is_dirty()||self.preview) {self.apply_vip_profile(tx);}
        else if !self.cust_prefs.busy(){self.reload_customers();self.apply_vip_profile(tx);}
    }
    fn apply_vip_profile(&mut self,tx:&mut Tx) {
        if !self.cust_prefs.loaded || self.cust_prefs.busy() || self.cust_prefs.error.is_some() {return;}
        let Some(login)=self.vip_profile_target.take() else{return;};
        let existing=self.cust_prefs.rows.iter().position(|p|p.login.eq_ignore_ascii_case(&login));
        let index=if let Some(index)=existing {index}else{
            self.cust_prefs.add=TextEdit::new(&login);self.cust_prefs.add();
            let Some(index)=self.cust_prefs.rows.iter().position(|p|p.login.eq_ignore_ascii_case(&login)) else{return;};index
        };
        self.cust_prefs.notice=Some(if existing.is_some(){format!("Editing VIP {}.",self.cust_prefs.rows[index].login)}else{format!("Add notes and choose a highlight for {login}, then save changes.")});
        tx.scroll_into_view(format!("customer-{}",self.cust_prefs.rows[index].login));
        tx.focus(format!("vip-notes-{index}"),true);tx.instant();
    }
    fn reload_customers(&mut self){
        if self.demo{showcase::vip_roster(self);return;}
        if self.cust_prefs.saving||self.cust_prefs.importing{return;}
        self.cust_prefs.loading=true;self.cust_prefs.error=None;
        if self.preview{self.cust_prefs.load(tracker_core::customer_config::CustomerSettings{customers:serde_json::from_str::<tracker_core::customer_config::CustomerFile>("{\"customers\":[]}").map(|f|f.customers).unwrap_or_default(),path:"vips.json".into(),..Default::default()});}
        else{self.cust_settings.request((),|_|data::customer_settings());}
    }
    fn apply_vips(&mut self, settings:tracker_core::customer_config::CustomerSettings) {
        let people=&settings.customers;
        self.data.vip_logins=people.iter().map(|p|p.login.clone()).collect();
        for item in self.items.iter_mut().chain(self.dev.items.iter_mut())
            .chain(self.data.pens.iter_mut().flat_map(|p|p.items.iter_mut()))
            .chain(self.data.pending.items.iter_mut()).chain(self.detail.iter_mut()) {
            if let Some(author)=item.author.as_mut() {
                let person=people.iter().find(|p|p.login.eq_ignore_ascii_case(&author.login));
                author.customer=person.is_some();
                author.full_name=person.map(|p|p.full_name.clone()).unwrap_or_default();
                author.company=person.map(|p|p.company.clone()).unwrap_or_default();
                author.notes=person.map(|p|p.notes.clone()).unwrap_or_default();
                author.highlight=person.map(|p|p.highlight.key().to_owned()).unwrap_or_default();
            }
        }
        self.cust_prefs.load(settings);
    }
    fn save_customers(&mut self){
        if self.cust_prefs.busy(){return;}self.cust_prefs.saving=true;self.cust_prefs.error=None;
        if self.preview{let customers=self.cust_prefs.customers();if let Some(saved)=self.demo_saved.as_mut(){saved.vips=customers.clone();}let settings=tracker_core::customer_config::CustomerSettings{customers,..Default::default()};self.apply_vips(settings);return;}
        self.cust_saved.request((self.cust_prefs.customers(),self.cust_prefs.revision.clone()),|(people,revision)|data::save_customers(people,revision));
    }
    fn import_customer_file(&mut self,tx:&mut Tx){
        if self.cust_prefs.busy(){return;}
        tx.open_file(rune::FileDialogOptions{title:"Import VIP roster".into(),..Default::default()}.filter("VIP roster",&["json"]),|s:&mut State,result,_|match result{
            Ok(Some(path))=>{s.cust_prefs.importing=true;s.cust_prefs.error=None;s.cust_imports.request(path,preferences::read_customer_import);},
            Ok(None)=>{},Err(error)=>s.cust_prefs.error=Some(error.to_string()),
        });
    }
    fn repositories(&self) -> Vec<String> {
        let mut repos=self.data.repositories.clone();
        repos.extend(self.items.iter().map(|item|item.repo.clone()));
        repos.sort();repos.dedup();repos
    }
    fn set_repo_scope(&mut self,scope:RepoScope) {
        self.repo_scope=scope;
        self.scope_error=if self.preview {None}else{self.repo_scope.save(&tracker_core::config::data_dir().join("repository-scope.json")).err()};
        self.refilter();
    }
    fn toggle_repo(&mut self,repo:&str){let mut scope=self.repo_scope.clone();scope.toggle(repo);self.set_repo_scope(scope);}
    fn matches_filters(&self, item: &Item) -> bool {
        let search = self.search.text.to_lowercase();
        (if self.dev.active {developer::matches(item,&self.dev.queue,self.dev.closed)}else{self.repo_scope.includes(&item.repo)}) && (self.kind.is_empty() || self.kind == item.kind)
            && (search.is_empty() || format!("{} {} {} #{}", item.title, item.repo, item.author.as_ref().map(|a| a.login.as_str()).unwrap_or(""), item.number).to_lowercase().contains(&search))
    }
    fn rebuild(&mut self) {
        self.visible = self.items.iter().enumerate().filter(|(_, item)| {
            (self.dev.active || self.pen.is_empty() || self.data.pens.iter().any(|p| p.id == self.pen && p.items.iter().any(|i| i.id == item.id)))
            && self.matches_filters(item)
        }).map(|(i, _)| i).collect();
        self.visible.sort_by(|a, b| {
            let order = (if self.dev.active {developer::sort_at(&self.items[*a],&self.dev.queue)}else{&self.items[*a].engagement.at}).cmp(if self.dev.active {developer::sort_at(&self.items[*b],&self.dev.queue)}else{&self.items[*b].engagement.at}).then(self.items[*a].id.cmp(&self.items[*b].id));
            if self.newest { order.reverse() } else { order }
        });
        if !self.visible.iter().any(|i| self.items[*i].id == self.selected) { self.selected = self.visible.first().map(|i| self.items[*i].id.clone()).unwrap_or_default(); }
    }
    // Poll checkpoints and sync indicators change frequently without changing any cards.
    fn apply_snapshot(&mut self, data:Snapshot)->bool {
        if data.backend.auth_failed && !self.auth.disconnected {self.auth.disconnected=true;self.auth.reconnect_open=true;self.auth.started=true;self.auth.error=Some("Your GitHub token expired or was revoked. Connect again to resume syncing. Your saved data is still available.".into());}
        let changed=self.data!=data || !matches!(self.connection,Connection::Online);
        self.connection=Connection::Online;
        let scope_changed=self.data.repositories!=data.repositories;
        if scope_changed {self.dev.loaded_queues.clear();self.dev.items.retain(|item|data.repositories.iter().any(|repo|repo.eq_ignore_ascii_case(&item.repo)));self.dev.next=0.0;}
        let entries_changed=(self.dev.active&&scope_changed)||self.data.pens!=data.pens || self.data.pending!=data.pending;
        if entries_changed {
            let active=self.active_id().to_owned();
            self.remember_note();
            self.items=if self.dev.active {self.dev.items.clone()}else{data.items()};
            self.data=data;
            self.rebuild();
            if self.active_id()!=active {self.restore_note();}
            // Connections can change when another entry is imported, even if this card did not.
            self.load_detail();
        } else {
            self.data=data;
            if self.detail.as_ref().is_none_or(|item|item.id!=self.active_id()) && !self.active_id().is_empty() {
                self.load_detail();
            }
        }
        changed
    }
    fn apply_detail(&mut self,item:Item)->bool {
        let recovered=self.detail_error.take().is_some();
        if self.detail.as_ref()==Some(&item) {
            // A cached detail can be identical on first launch while its comments are not loaded yet.
            if (item.comments>0 || item.kind=="pr") && self.discussions.get(&item.id).is_none_or(|d|!d.loaded&&!d.loading&&d.error.is_none()) {
                self.load_discussion(1,false);
                return recovered || self.discussions.get(&item.id).is_some_and(|d|d.loading);
            }
            return recovered;
        }
        let advice_changed=self.note.text!=item.advice.text || self.note_enabled!=item.advice.enabled;
        self.detail=Some(item);
        if advice_changed && !matches!(self.note_state,NoteState::Dirty|NoteState::Saving) {self.restore_note();}
        self.load_discussion(1,false);
        true
    }
    fn refilter(&mut self) { self.remember_note(); self.preview_id=None; self.detail_error=None; self.rebuild(); self.restore_note(); self.load_detail(); }
    fn select(&mut self, id: String) { if id != self.selected || self.preview_id.is_some() { self.remember_note(); self.preview_id=None; self.detail_error=None; self.selected = id; self.restore_note(); self.load_detail(); } }
    fn selected_item(&self) -> Option<&Item> { self.items.iter().find(|i| i.id == self.selected) }
    fn active_id(&self) -> &str { self.preview_id.as_deref().unwrap_or(&self.selected) }
    fn current_item(&self) -> Option<&Item> {
        self.detail.as_ref().filter(|i|i.id==self.active_id()).or_else(||self.items.iter().find(|i|i.id==self.active_id()))
    }
    fn preview_connection(&mut self, id:String) {
        if id==self.active_id(){return;}
        self.remember_note();self.preview_id=Some(id);self.detail_error=None;self.restore_note();self.load_detail();
    }
    fn back_to_original(&mut self) {
        self.remember_note();self.preview_id=None;self.detail_error=None;self.restore_note();self.load_detail();
    }
    fn remember_note(&mut self) { if matches!(self.note_state, NoteState::Dirty) { self.drafts.insert(self.active_id().into(), (self.note.text.clone(), self.note_enabled)); } }
    fn restore_note(&mut self) {
        if let Some((text, enabled)) = self.drafts.get(self.active_id()) {
            if self.note.text!=*text {self.note = TextEdit::new(text);} self.note_enabled = *enabled; self.note_state = NoteState::Dirty;
        } else {
            let advice = self.current_item().map(|i| i.advice.clone()).unwrap_or_default();
            if self.note.text!=advice.text {self.note = TextEdit::new(&advice.text);} self.note_enabled = advice.enabled; self.note_state = NoteState::Clean;
        }
    }
    fn load_discussion(&mut self, page:u64, force:bool) {
        let Some(item)=self.current_item() else{return;};
        if self.preview {return;}
        if item.comments==0 && item.kind!="pr" {self.discussions.remove(&self.active_id().to_owned());return;}
        let id=item.id.clone();let version=item.updated_at.clone();
        for (key,state) in &mut self.discussions {if key!=&id {state.loading=false;}}
        let state=self.discussions.entry(id.clone()).or_default();
        if state.loading {return;}
        if page==1 && !force && state.version==version && (state.loaded||state.error.is_some()) {return;}
        state.loading=true;state.error=None;
        self.discussion_results.request((id,page,force,version),|(id,page,force,version)|{let result=data::discussion(&id,page,force);(id,page,version,result)});
    }
    fn load_detail(&self) { if !self.preview && !self.auth_blocked() && !self.active_id().is_empty() { self.details.request(self.active_id().to_owned(), |id| { let result = data::detail(&id); (id, result) }); } }
    fn refresh(&self) { if !self.preview && !self.auth_blocked() { self.snapshots.request((), |_| data::snapshot()); } }
    fn hard_refresh(&mut self) {
        if self.demo{return;}
        if self.dev.active {self.load_developer(true);return;}
        let selected=self.repo_scope.selected(&self.repositories());
        if self.refreshing || selected.is_empty() {return;}
        self.refreshing=true;self.refresh_at=None;self.refresh_dismiss_at=None;
        self.refresh_status=Some(data::RefreshStatus {status:"running".into(),message:"Requesting GitHub refresh…".into(),repo:(selected.len()==1).then(||selected[0].clone()),repositories:Some(selected.clone())});
        if !self.preview {self.github_refresh.request(selected,data::github_refresh);}
    }
    fn save(&mut self) {
        if self.active_id().is_empty() { return; }
        if self.preview { self.note_state = NoteState::Saved; return; }
        self.note_state = NoteState::Saving;
        self.saves.request((self.active_id().to_owned(), self.note.text.clone(), true), |(id,text,enabled)| { let result = data::save_advice(&id,&text,enabled); (id,result) });
    }
    fn move_selection(&mut self, delta: isize, tx: &mut Tx) {
        if self.visible.is_empty() { return; }
        let position = self.visible.iter().position(|i| self.items[*i].id == self.selected).unwrap_or(0);
        let position = (position as isize + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        self.select(self.items[self.visible[position]].id.clone());
        tx.scroll_into_view(format!("entry-{}", self.selected));
    }
}
fn setup(s: &mut State, waker: Waker) { s.workspace.result.set_waker(waker.clone()); s.runtime_waker=Some(waker.clone());s.dev.results.set_waker(waker.clone());s.dev.profiles_result.set_waker(waker.clone());s.auth.discovered.set_waker(waker.clone());s.auth.connected.set_waker(waker.clone()); s.cust_settings.set_waker(waker.clone());s.cust_saved.set_waker(waker.clone());s.cust_imports.set_waker(waker.clone());s.maint_settings.set_waker(waker.clone());s.maint_saved.set_waker(waker.clone());s.maint_imports.set_waker(waker.clone()); s.snapshots.set_waker(waker.clone()); s.details.set_waker(waker.clone()); s.discussion_results.set_waker(waker.clone()); s.saves.set_waker(waker.clone());s.github_refresh.set_waker(waker.clone());s.repo_settings.set_waker(waker.clone());s.repo_imports.set_waker(waker.clone());s.repo_settings_saved.set_waker(waker.clone());if !s.preview&&!s.settings_window {s.auth.discover();} }
fn poll(s: &mut State, now: f64, tx: &mut Tx) -> bool {
    let was_blocked=s.auth_blocked();
    let mut auth_changed=false;
    if let Some(result)=s.auth.discovered.take(){
        s.auth.checking=false;auth_changed=true;
        match result{Ok(Some(token))=>s.connected(token),Ok(None)=>{},Err(error)=>s.auth.error=Some(error)}
    }
    if let Some(result)=s.auth.connected.take(){
        s.auth.saving=false;auth_changed=true;
        match result{Ok(token)=>s.connected(token),Err(error)=>s.auth.error=Some(error)}
    }
    if s.auth_blocked(){if auth_changed{tx.instant();}return auth_changed;}

    if was_blocked && (!s.preview||s.demo) && !s.settings_window {tx.resize_window((1380.0,880.0));tx.center_window();}

    if s.settings_window&&!s.repo_prefs.open{tx.close_window();return false;}
    let mut changed = auth_changed;
    changed |= s.poll_developer(now);
    if let Some(result)=s.maint_settings.take(){match result{Ok(settings)=>s.maint_prefs.load(settings),Err(error)=>{s.maint_prefs.loading=false;s.maint_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.cust_settings.take(){match result{Ok(settings)=>{if s.settings_window{s.cust_prefs.load(settings);}else{s.apply_vips(settings);}},Err(error)=>{s.cust_prefs.loading=false;s.cust_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.maint_saved.take(){match result{Ok(settings)=>{s.maint_prefs.load(settings);s.refresh();},Err(error)=>{s.maint_prefs.saving=false;s.maint_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.cust_saved.take(){match result{Ok(settings)=>{s.apply_vips(settings.clone());if s.settings_window && let Some(app)=MAIN_APP.get(){app.update(move|s,tx|{s.apply_vips(settings);s.refresh();tx.instant();});}s.refresh();},Err(error)=>{s.cust_prefs.saving=false;s.cust_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.maint_imports.take(){match result{Ok(people)=>s.maint_prefs.import(people),Err(error)=>{s.maint_prefs.importing=false;s.maint_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.cust_imports.take(){match result{Ok(people)=>s.cust_prefs.import(people),Err(error)=>{s.cust_prefs.importing=false;s.cust_prefs.error=Some(error);}}changed=true;}

    if s.vip_profile_target.is_some(){s.apply_vip_profile(tx);}

    if let Some((id,page,version,result))=s.discussion_results.take(){
        let state=s.discussions.entry(id).or_default();state.loading=false;state.version=version;
        match result {
            Ok(data)=>{state.loaded=true;if page==1{state.comments.clear();}for comment in data.comments{if let Some(old)=state.comments.iter_mut().find(|c|c.id==comment.id && c.kind==comment.kind){*old=comment;}else{state.comments.push(comment);}}state.comments.sort_by(|a,b|a.created_at.cmp(&b.created_at));state.next=data.next;state.warning=data.warning;state.error=None;},
            Err(error)=>state.error=Some(error),
        }
        changed=true;
    }
    if let Some(result)=s.repo_imports.take(){match result{Ok(repos)=>s.repo_prefs.import(repos),Err(error)=>{s.repo_prefs.importing=false;s.repo_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.repo_settings.take(){match result{Ok(settings)=>s.repo_prefs.load(settings),Err(error)=>{s.repo_prefs.loading=false;s.repo_prefs.error=Some(error);}}changed=true;}
    if let Some(result)=s.repo_settings_saved.take(){match result{Ok(settings)=>{s.repo_prefs.load(settings);if !s.settings_window{s.repo_prefs.open=false;}s.refresh();},Err(error)=>{s.repo_prefs.saving=false;s.repo_prefs.error=Some(error);}}changed=true;}
    changed |= s.poll_workspace();

    if let Some(result)=s.github_refresh.take() {
        match result {
            Ok(status) => {
                s.refreshing=status.status=="running";
                s.refresh_at=s.refreshing.then_some(now+2.0);
                s.refresh_dismiss_at=(status.status=="complete").then_some(now+15.0);
                s.refresh_status=Some(status);
                s.refresh();
            }
            Err(message) => {s.refreshing=false;s.refresh_at=None;s.refresh_dismiss_at=None;s.refresh_status=Some(data::RefreshStatus {status:"failed".into(),message,repo:None,repositories:None});s.refresh();}
        }
        changed=true;
    }
    if let Some(result) = s.snapshots.take() {
        match result {
            Ok(data) => changed |= s.apply_snapshot(data),
            Err(message) => {s.connection = Connection::Offline(message);changed=true;},
        }
    }
    if let Some((id, result)) = s.details.take() && id == s.active_id() {
        match result {
            Ok(item)=>changed |= s.apply_detail(item),
            Err(error)=>{changed |= s.detail_error.as_ref()!=Some(&error);s.detail_error=Some(error);},
        }
    }
    if let Some((id,result)) = s.saves.take() {
        match result {
            Ok(advice) => {
                s.drafts.remove(&id);
                if let Some(item) = s.items.iter_mut().find(|i| i.id == id) { item.advice = advice.clone(); }
                if let Some(item)=s.dev.items.iter_mut().find(|i|i.id==id){item.advice=advice.clone();}
                for item in s.data.pens.iter_mut().flat_map(|pen|pen.items.iter_mut()).chain(s.data.pending.items.iter_mut()){if item.id==id{item.advice=advice.clone();}}
                if let Some(item) = s.detail.as_mut().filter(|i| i.id == id) { item.advice = advice; }
                if id == s.active_id() { s.note_state = NoteState::Saved; }
            }
            Err(message) => if id == s.active_id() { s.note_state = NoteState::Failed(message); },
        }
        changed = true;
    }
    if let Some(deadline)=s.refresh_dismiss_at {
        if now>=deadline {s.refresh_status=None;s.refresh_dismiss_at=None;changed=true;}
        else {tx.wake_after(Duration::from_secs_f64(deadline-now));}
    }
    if !s.preview {
        let revision=data::revision();if s.revision!=revision {s.revision=revision;s.refresh();}
        if now >= s.next_refresh {
            s.refresh();
            // Closed connected previews need a periodic local check too; equal results are ignored.
            s.load_detail();
            if s.dev.active {s.load_developer(false);} else {s.load_developer_cache();}
            s.next_refresh = now + 60.0;
            changed=true; // Relative ages can change even when the data does not.
        }
        if s.refresh_at.is_some_and(|at|now>=at) {
            s.refresh_at=None;s.github_refresh.request((),|_|data::refresh_status());
        }
        let next=s.refresh_at.map_or(s.next_refresh,|at|at.min(s.next_refresh));
        tx.wake_after(Duration::from_secs_f64((next - now).max(0.1)));
    }
    if changed { tx.instant(); }
    changed
}
fn key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if s.auth.reconnect_open && matches!(e.key,Key::Escape|Key::Command('w')){s.auth.reconnect_open=false;tx.instant();return true;}
    if s.auth_blocked() || s.auth.reconnect_open {
        match e.key {Key::Command('q')|Key::Command('w')=>tx.quit(),Key::Command(','|'r'|'f'|'s'|'o')|Key::Escape=>return true,_=>return false}
        return true;
    }
    if matches!(e.key, Key::Command('f')) && e.mods.ctrl && !e.mods.alt && !e.mods.shift {
        if !s.settings_window {tx.toggle_full_screen();}
        return true;
    }
    if s.settings_close_confirm {return match e.key {Key::Escape=>{s.settings_close_confirm=false;true},Key::Command('w'|'s'|',')=>true,_=>false};}
    if s.repo_prefs.open {
        match &e.key {
            Key::Command('s')=>{if s.settings_tab=="workspace"{s.apply_workspace();}else if s.settings_tab=="customers"{s.save_customers();}else if s.settings_tab=="maintainers"{s.save_maintainers();}else{s.save_repository_preferences();}},
            Key::Command(',')=>{},
            Key::Escape|Key::Command('w')=>s.request_settings_close(),
            _=>return false,
        }
        return true;
    }
    match &e.key {
        Key::Command(',') => s.open_repository_preferences(),
        Key::Command('r') => s.hard_refresh(),
        Key::Command('f') => tx.focus("search", true),
        Key::Command('s') => s.save(),
        Key::Command('o') => if let Some(item) = s.current_item() { if item.url.starts_with("https://github.com/") { let _ = tx.open_url(&item.url); } },
        Key::Down if tx.focused().is_none() || tx.focused().is_some_and(|key| key.contains("entry-")) => s.move_selection(1, tx),
        Key::Up if tx.focused().is_none() || tx.focused().is_some_and(|key| key.contains("entry-")) => s.move_selection(-1, tx),
        Key::Escape if s.repo_prefs.open => s.request_settings_close(),
        Key::Escape if s.repo_picker_open => {s.repo_picker_open=false;tx.focus("repository-picker",false);}
        Key::Escape => { s.search = TextEdit::new(""); s.refilter(); tx.focus("entries", false); }
        _ => return false,
    }
    true
}
fn main() {
    if std::env::args().any(|arg|arg=="--check-backend") {
        let result=data::snapshot().and_then(|snapshot| {
            println!("Embedded backend: {} external entries, {} repositories",snapshot.total,snapshot.repositories.len());
            if let Some(item)=snapshot.items().first() {let detail=data::detail(&item.id)?;println!("Entry detail OK: {} #{}",detail.repo,detail.number);}
            Ok(())
        });
        data::shutdown();
        if let Err(error)=result {eprintln!("{error}");std::process::exit(1);}
        return;
    }
    let spec = AppSpec { min_size: Some((640.0, 680.0)), families: (".SF NS", "Menlo"),
        setup: Some(setup), poll: Some(poll), on_key: Some(key),
        scenes: &[("workspace-import",|s|{s.connected("mock_token".into());s.open_repository_preferences();s.settings_page("workspace");s.workspace.draft=Some(workspace::Bundle::parse(include_str!("../../workspace.example.json")).unwrap());}),("demo-start",|s|{s.preview=true;s.auth=auth::Setup::default();}),("demo",|s|s.connected("mock_token".into())),("showcase-comments",showcase::comments),("showcase-vips",showcase::vips),("showcase-followup",showcase::followup),("showcase-developer",showcase::work),("developer-paused",|s|{developer::preview(s);s.dev.items.clear();s.items.clear();s.dev.loaded_queues.clear();s.dev.checked=None;s.dev.paused=true;s.dev.retry_at=Some(chrono::Utc::now().timestamp_millis()+600_000);s.dev.error=Some("Showing saved results where available. Navigation and Refresh respect the cooldown.".into());s.developer_queue("participating");}),("developer-work",developer::preview),("developer-settings",|s|{s.open_repository_preferences();s.settings_page("developer");}),("reconnect",|s|{s.auth.ready=true;s.auth.started=true;s.auth.disconnected=true;s.auth.reconnect_open=true;s.auth.error=Some("Your GitHub token expired or was revoked. Connect again to resume syncing. Your saved data is still available.".into());}),("welcome",|s|{s.auth=auth::Setup::default();}),("github-setup-long-token",|s|{s.auth=auth::Setup::default();s.auth.started=true;s.auth.token=TextEdit::new(&"x".repeat(200)).secure();}),("github-setup",|s|{s.auth=auth::Setup::default();s.auth.started=true;}),("long-note", |s|{s.note=TextEdit::new(&"A saved note with several lines of context.\n".repeat(300));s.note_state=NoteState::Dirty;}), ("connection-preview", |s|{if let Some(item)=s.items.iter().find(|i|i.id!=s.selected){s.preview_connection(item.id.clone());}}), ("discussion", |s| {
            s.show_advice=false;
            if let Some(mut item)=s.selected_item().cloned(){
                item.body="Example description for the discussion preview.".into();item.comments=2;
                item.related=vec![data::Related{repo:item.repo.clone(),number:item.number,title:Some("Example connected entry".into()),url:item.url.clone(),..Default::default()}];
                s.discussions.insert(item.id.clone(),DiscussionState{loaded:true,comments:vec![
                    data::Comment{kind:"comment".into(),review_state:String::new(),id:1,author:"example-contributor".into(),body:"I can reproduce this on the latest SDK. Here are the steps I tried.".into(),created_at:"2026-11-09T09:00:00Z".into(),updated_at:"2026-11-09T09:00:00Z".into(),url:item.url.clone()},
                    data::Comment{kind:"comment".into(),review_state:String::new(),id:2,author:"example-maintainer".into(),body:"Thanks for the details. We are investigating and will follow up here.".into(),created_at:"2026-11-10T09:00:00Z".into(),updated_at:"2026-11-10T10:00:00Z".into(),url:item.url.clone()},
                ],..Default::default()});s.detail=Some(item);
            }
        }), ("dark", |s| s.appearance = Some(true)), ("light", |s| s.appearance = Some(false)), ("week", |s| { s.pen = "week".into(); s.rebuild(); s.restore_note(); }), ("prs", |s| { s.kind = "pr".into(); s.rebuild(); s.restore_note(); }), ("refreshing", |s| s.hard_refresh()), ("repo-picker",|s|s.repo_picker_open=true), ("preferences",|s|s.open_repository_preferences()), ("repo-exclusions",|s|{s.repo_scope=RepoScope::AllExcept(s.repositories().into_iter().take(2).collect());s.repo_picker_open=true;s.rebuild();s.restore_note();}), ("scoped", |s| {s.repo_scope=RepoScope::Only(s.repositories().first().cloned().into_iter().collect());s.pen="day".into();s.rebuild();s.restore_note();})],
        ..AppSpec::new("Threadkeeper — Follow-up", (680.0, 760.0), State::init, ui::view)
    };
    let mut app = App::new(spec).titlebar(Titlebar::new(60.0))
        .window_options(WindowOptions { transparent: true, ..Default::default() }).standard_menus().on_close(|s,tx|{if s.auth_blocked(){tx.quit();}});
    let settings_spec=AppSpec{min_size:Some((920.0,680.0)),families:(".SF NS","Menlo"),setup:Some(setup),poll:Some(poll),on_key:Some(key),scenes:&[("maintainers",|s|s.settings_page("maintainers"))],..AppSpec::new("Threadkeeper — Settings",(960.0,720.0),State::settings_init,ui::view)};
    // MoveToActiveSpace | FullScreenAuxiliary: Settings accompanies the main window,
    // including its full-screen Space, rather than becoming another primary window.
    let settings_options=WindowOptions{visible:false,..Default::default()}.macos(
        rune::MacWindowOptions::default().collection_behavior((1 << 1) | (1 << 8))
    );
    let settings=app.window(rune::WindowSpec::from_app("settings",App::new(settings_spec).on_close(|s,_|{if !s.repo_prefs.open||rune::quitting(){return;}s.request_settings_close();if s.repo_prefs.open && let Some((_,window))=SETTINGS_WINDOW.get(){window.open();}})).titlebar(Titlebar::new(60.0)).configured_options(settings_options));
    if !std::env::args().any(|a|["--snap","--tree","--script","--list-scenes"].contains(&a.as_str())){let _=SETTINGS_WINDOW.set(settings);}
    let handle = app.handle();
    let _=MAIN_APP.set(handle.clone());
    app.on_ready(move |_| { handle.update(|s, tx| { s.refresh();if s.demo{tx.resize_window((1380.0,880.0));tx.center_window();}tx.instant(); }); }).run();
    data::shutdown();
}

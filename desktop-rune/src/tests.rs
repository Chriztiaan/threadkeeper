use super::*;
use rune::{Env, Runtime, Mods};

fn fixture() -> State {
    let data:Snapshot=serde_json::from_str(r#"{"total":3,"maintainerCount":17,"pens":[{"id":"week","label":"Last week","total":3,"items":[
        {"id":"1","number":1,"title":"Offline writes disappear","type":"issue","state":"open","repo":"acme/sdk","author":{"id":"7","login":"alice"},"assignees":[{"id":8,"login":"bob"}],"engagement":{"at":"2026-11-01T12:00:00Z","kind":"comment"},"advice":{"text":"Ask for a reproduction","enabled":true}},
        {"id":"2","number":2,"title":"Preserve pending writes","type":"pr","state":"draft","repo":"acme/sdk","author":{"login":"bob"},"engagement":{"at":"2026-11-02T12:00:00Z","kind":"review"}},
        {"id":"3","number":3,"title":"Improve reconnect errors","type":"issue","state":"open","repo":"acme/core","author":{"login":"casey"},"engagement":{"at":"2026-11-03T12:00:00Z","kind":"created"}}
    ]}]}"#).unwrap();
    let mut s=State::init();s.preview=true;s.auth.ready=true;s.repo_scope=RepoScope::default();s.repo_picker_open=false;s.items=data.items();s.data=data;s.detail=None;s.selected.clear();s.refilter();s
}
fn click(rt:&mut Runtime<State>,key:&str){let(x,y)=rt.find(key).expect(key);rt.pointer_down(x,y);rt.pointer_up(x,y);rt.frame(0.0);}
#[test]
fn filters_and_appearance_work_in_the_retained_view() {
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"type-Pull requests");assert_eq!(rt.state.kind,"pr");assert_eq!(rt.state.visible.len(),1);assert_eq!(rt.state.selected,"2");
    click(&mut rt,"appearance");assert_eq!(rt.state.appearance,Some(true));
    click(&mut rt,"type-All");assert_eq!(rt.state.visible.len(),3);
    click(&mut rt,"repository-picker");click(&mut rt,"repo-only-acme/core");click(&mut rt,"repo-picker-done");assert_eq!(rt.state.visible.len(),1);assert_eq!(rt.state.selected,"3");
}
#[test]
fn pen_counts_share_list_filters_and_reset_preserves_navigation() {
    let mut s=fixture();s.pen="week".into();s.repo_scope=RepoScope::only("acme/sdk");s.kind="pr".into();s.search=TextEdit::new("pending");s.refilter();
    let count=s.data.pens[0].items.iter().filter(|i|i.author.is_some()&&s.matches_filters(i)).count();
    assert_eq!(count,1);assert_eq!(count,s.visible.len());
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("active-filters").is_some());click(&mut rt,"reset-list-filters");
    assert_eq!(rt.state.pen,"week");assert_eq!(rt.state.repo_scope,RepoScope::only("acme/sdk"));assert!(rt.state.kind.is_empty());assert!(rt.state.search.text.is_empty());assert_eq!(rt.state.visible.len(),2);
    assert!(rt.find("reset-list-filters").is_none());
}
#[test]
fn unknown_authors_are_included_in_the_regular_pens() {
    let mut s=fixture();s.data.pens[0].items[0].author=None;s.items=s.data.items();s.pen="week".into();s.refilter();
    assert_eq!(s.visible.len(),3);assert!(s.visible.iter().any(|i|s.items[*i].author.is_none()));
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("nav-unknown").is_none());
}
#[test]
fn search_order_and_keyboard_navigation_share_the_same_selection() {
    let mut s=fixture();s.search=TextEdit::new("pending");s.refilter();assert_eq!(s.selected,"2");assert_eq!(s.visible.len(),1);
    s.search=TextEdit::new("");s.refilter();s.selected="1".into();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"entry-1");rt.key(KeyEvent{key:Key::Down,mods:Mods::default()});rt.frame(0.0);assert_eq!(rt.state.selected,"2");
    click(&mut rt,"order");assert!(rt.state.newest);assert_eq!(rt.state.items[rt.state.visible[0]].id,"3");
}
#[test]
fn advice_drafts_survive_selection_changes_and_ui_toggle_does_not_write() {
    let mut s=fixture();s.note=TextEdit::new("Unsaved advice");s.note_state=NoteState::Dirty;s.select("2".into());s.select("1".into());assert_eq!(s.note.text,"Unsaved advice");
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"toggle-advice");assert!(!rt.state.show_advice);
    click(&mut rt,"toggle-advice");click(&mut rt,"save-advice");assert!(matches!(rt.state.note_state,NoteState::Saved));
}

#[test]
fn repository_scope_and_pen_navigation_remain_independent() {
    let mut s=fixture();s.pen="day".into();s.repo_scope=RepoScope::only("acme/core");s.refilter();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert_eq!(rt.state.visible.len(),0);
    click(&mut rt,"view-all-followup");assert!(rt.state.pen.is_empty());assert_eq!(rt.state.repo_scope,RepoScope::only("acme/core"));assert_eq!(rt.state.visible.len(),1);
    click(&mut rt,"nav-week");assert_eq!(rt.state.repo_scope,RepoScope::only("acme/core"));assert_eq!(rt.state.pen,"week");
    click(&mut rt,"repository-picker");click(&mut rt,"repo-select-all");click(&mut rt,"repo-picker-done");
    assert_eq!(rt.state.pen,"week");assert_eq!(rt.state.repo_scope,RepoScope::default());assert_eq!(rt.state.visible.len(),3);
    assert!(rt.find("repositories").is_none());
}
#[test]
fn repository_picker_includes_configured_repositories_with_no_entries() {
    let mut s=fixture();s.data.repositories=vec!["acme/empty".into()];s.kind="pr".into();s.refilter();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"repository-picker");click(&mut rt,"repo-only-acme/empty");click(&mut rt,"repo-picker-done");
    assert_eq!(rt.state.repo_scope,RepoScope::only("acme/empty"));assert_eq!(rt.state.kind,"pr");assert!(rt.state.visible.is_empty());
}

#[test]
fn manual_refresh_preserves_scope_and_prevents_duplicate_clicks() {
    let mut s=fixture();s.repo_scope=RepoScope::only("acme/sdk");s.pen="week".into();s.refilter();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"refresh");assert!(rt.state.refreshing);
    assert_eq!(rt.state.refresh_status.as_ref().unwrap().repo.as_deref(),Some("acme/sdk"));
    rt.state.repo_scope=RepoScope::only("acme/core");
    rt.key(KeyEvent{key:Key::Command('r'),mods:Mods::default()});rt.frame(0.0);
    assert_eq!(rt.state.refresh_status.as_ref().unwrap().repo.as_deref(),Some("acme/sdk"));
    assert!(rt.find("github-refresh-status").is_some());assert_eq!(rt.state.pen,"week");
}

#[test]
fn repository_checkboxes_keep_popover_open_and_counts_match_the_list() {
    let mut s=fixture();s.pen="week".into();s.refilter();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"repository-picker");click(&mut rt,"repo-check-acme/core");
    assert!(rt.state.repo_picker_open);assert_eq!(rt.state.visible.len(),2);assert!(!rt.state.repo_scope.includes("acme/core"));
    let count=rt.state.data.pens[0].items.iter().filter(|i|rt.state.matches_filters(i)).count();assert_eq!(count,rt.state.visible.len());
    click(&mut rt,"repo-check-acme/sdk");assert_eq!(rt.state.visible.len(),0);
    click(&mut rt,"repo-select-all");assert_eq!(rt.state.visible.len(),3);assert!(rt.state.repo_picker_open);
    rt.key(KeyEvent{key:Key::Escape,mods:Mods::default()});rt.frame(0.0);assert!(!rt.state.repo_picker_open);
}
#[test]
fn repository_search_is_independent_and_outside_click_dismisses() {
    let mut s=fixture();s.search=TextEdit::new("writes");s.refilter();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);click(&mut rt,"repository-picker");
    rt.update(|s,_|s.repo_search=TextEdit::new("core"));rt.frame(0.0);
    assert!(rt.find("repo-check-acme/core").is_some());assert!(rt.find("repo-check-acme/sdk").is_none());assert_eq!(rt.state.search.text,"writes");assert_eq!(rt.state.visible.len(),2);
    rt.pointer_down(1000.0,700.0);rt.pointer_up(1000.0,700.0);rt.frame(0.0);assert!(!rt.state.repo_picker_open);assert_eq!(rt.state.search.text,"writes");
}
#[test]
fn deselect_all_has_an_explicit_empty_state_and_cannot_refresh_everything() {
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"repository-picker");click(&mut rt,"repo-deselect-all");click(&mut rt,"repo-picker-done");assert_eq!(rt.state.visible.len(),0);assert!(rt.find("choose-repositories").is_some());
    rt.state.hard_refresh();assert!(!rt.state.refreshing);
    rt.frame(0.4);click(&mut rt,"choose-repositories");assert!(rt.state.repo_picker_open);
}
#[test]
fn persisted_scope_preserves_exclusions_and_explicit_subset_semantics() {
    let file=std::env::temp_dir().join(format!("powersync-repo-scope-{}.json",std::process::id()));
    let mut scope=RepoScope::default();scope.toggle("acme/core");scope.save(&file).unwrap();
    let restored=RepoScope::load(&file);assert_eq!(scope,restored);assert!(!restored.includes("acme/core"));assert!(restored.includes("acme/new"));
    let only=RepoScope::only("acme/sdk");only.save(&file).unwrap();let restored=RepoScope::load(&file);assert_eq!(restored,only);assert!(!restored.includes("acme/new"));
    std::fs::remove_file(file).unwrap();
}
#[test]
fn multi_repository_refresh_captures_selection_before_it_changes() {
    let mut s=fixture();s.repo_scope=RepoScope::Only(["acme/sdk".into(),"acme/core".into()].into());s.hard_refresh();
    let scope=s.refresh_status.as_ref().unwrap().repositories.clone().unwrap();assert_eq!(scope.len(),2);
    s.repo_scope=RepoScope::only("acme/core");s.hard_refresh();assert_eq!(s.refresh_status.as_ref().unwrap().repositories.as_ref().unwrap(),&scope);
}

#[test]
fn repository_preferences_add_urls_toggle_tracking_remove_and_cancel(){
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"manage-repositories");assert!(rt.state.repo_prefs.open);assert_eq!(rt.state.repo_prefs.rows.len(),2);
    rt.update(|s,_|s.repo_prefs.add=TextEdit::new("https://github.com/acme/new.git"));rt.frame(0.0);click(&mut rt,"add-tracked-repository");assert_eq!(rt.state.repo_prefs.rows.len(),3);assert_eq!(rt.state.repo_prefs.rows[2].name,"acme/new");
    click(&mut rt,"track-toggle-acme/new");assert!(!rt.state.repo_prefs.rows[2].enabled);
    click(&mut rt,"track-remove-acme/new");assert_eq!(rt.state.repo_prefs.rows.len(),2);assert!(rt.state.repo_prefs.dirty);
    click(&mut rt,"cancel-repository-preferences");assert!(!rt.state.repo_prefs.open);assert_eq!(rt.state.visible.len(),3);
}
#[test]
fn repository_preferences_validate_duplicate_and_invalid_names_and_have_keyboard_shortcut(){
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    rt.key(KeyEvent{key:Key::Command(','),mods:Mods::default()});rt.frame(0.0);assert!(rt.state.repo_prefs.open);
    rt.update(|s,_|s.repo_prefs.add=TextEdit::new("acme/sdk"));rt.frame(0.0);click(&mut rt,"add-tracked-repository");assert!(rt.state.repo_prefs.error.as_ref().unwrap().contains("already listed"));assert_eq!(rt.state.repo_prefs.rows.len(),2);
    rt.update(|s,_|s.repo_prefs.add=TextEdit::new("https://evil.example/acme/sdk"));rt.frame(0.0);click(&mut rt,"add-tracked-repository");assert_eq!(rt.state.repo_prefs.rows.len(),2);
    rt.key(KeyEvent{key:Key::Escape,mods:Mods::default()});rt.frame(0.0);assert!(!rt.state.repo_prefs.open);
}

#[test]
fn preferences_keyboard_save_keeps_advice_untouched_and_reopening_keeps_the_draft(){
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"manage-repositories");click(&mut rt,"track-remove-acme/sdk");assert_eq!(rt.state.repo_prefs.rows.len(),1);
    rt.key(KeyEvent{key:Key::Command(','),mods:Mods::default()});rt.frame(0.0);assert_eq!(rt.state.repo_prefs.rows.len(),1);
    rt.key(KeyEvent{key:Key::Command('s'),mods:Mods::default()});rt.frame(0.0);assert!(!rt.state.repo_prefs.open);assert!(matches!(rt.state.note_state,NoteState::Clean));
}

#[test]
fn native_import_picker_loads_a_reviewable_draft_without_changing_tracking(){
    let path=std::env::temp_dir().join(format!("tracker-import-{}.json",std::process::id()));
    let raw=r#"{"repositories":[{"name":"https://github.com/Acme/Imported.git","group":"Preserve this group","enabled":false}]}"#;
    std::fs::write(&path,raw).unwrap();
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.poll=Some(poll);rt.frame(0.0);
    click(&mut rt,"manage-repositories");assert!(rt.find("track-group-acme/sdk").is_none());assert!(rt.state.repo_prefs.rows[0].include_terms.text.is_empty());
    click(&mut rt,"import-repository-file");let(kind,options)=rt.file_dialog().unwrap();assert_eq!(kind,rune::FileDialogKind::Open);assert_eq!(options.filters[0].1,vec!["json"]);
    rt.resolve_file_dialog(Some(path.clone())).unwrap();rt.frame(0.0);
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
    while rt.state.repo_prefs.importing&&std::time::Instant::now()<deadline{std::thread::sleep(std::time::Duration::from_millis(5));rt.frame(0.0);}
    assert!(!rt.state.repo_prefs.importing);assert_eq!(rt.state.repo_prefs.rows.len(),1);assert_eq!(rt.state.repo_prefs.rows[0].name,"acme/imported");assert!(!rt.state.repo_prefs.rows[0].enabled);assert!(rt.state.repo_prefs.rows[0].include_terms.text.is_empty());assert!(rt.state.repo_prefs.dirty);assert!(rt.state.repo_prefs.notice.is_some());
    assert_eq!(rt.state.visible.len(),3);assert_eq!(std::fs::read_to_string(&path).unwrap(),raw);
    click(&mut rt,"cancel-repository-preferences");assert_eq!(rt.state.visible.len(),3);std::fs::remove_file(path).unwrap();
}
#[test]
fn cancelling_file_picker_and_failed_imports_leave_existing_draft_untouched(){
    let path=std::env::temp_dir().join(format!("tracker-bad-import-{}.json",std::process::id()));std::fs::write(&path,"not json").unwrap();
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.poll=Some(poll);rt.frame(0.0);click(&mut rt,"manage-repositories");
    click(&mut rt,"import-repository-file");rt.resolve_file_dialog(None).unwrap();rt.frame(0.0);assert_eq!(rt.state.repo_prefs.rows.len(),2);assert!(!rt.state.repo_prefs.dirty);
    click(&mut rt,"import-repository-file");rt.resolve_file_dialog(Some(path.clone())).unwrap();rt.frame(0.0);
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);while rt.state.repo_prefs.importing&&std::time::Instant::now()<deadline{std::thread::sleep(std::time::Duration::from_millis(5));rt.frame(0.0);}
    assert!(rt.state.repo_prefs.error.as_ref().unwrap().contains("Could not import"));assert_eq!(rt.state.repo_prefs.rows.len(),2);assert!(!rt.state.repo_prefs.dirty);
    std::fs::write(&path,r#"{"repositories":[{"name":"acme/sdk"},{"name":"ACME/sdk"}]}"#).unwrap();assert!(preferences::read_import(path.clone()).is_err());
    std::fs::write(&path,r#"{"pollIntervalSeconds":300,"repositories":[{"name":"acme/legacy"}]}"#).unwrap();let imported=preferences::read_import(path.clone()).unwrap();assert!(imported[0].include_terms.is_empty());assert!(imported[0].enabled);std::fs::remove_file(path).unwrap();
}

#[test]
fn notes_preserve_existing_text_and_no_longer_offer_an_indicator_checkbox() {
    let mut s=fixture();s.items[0].advice.enabled=false;s.restore_note();
    assert_eq!(s.note.text,"Ask for a reproduction");
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("advice-input").is_some());assert!(rt.find("advice-enabled").is_none());
    click(&mut rt,"toggle-advice");assert!(rt.find("advice-input").is_none());
    click(&mut rt,"toggle-advice");assert_eq!(rt.state.note.text,"Ask for a reproduction");
}

#[test]
fn comments_stay_with_their_entry_and_connections_have_separate_external_buttons() {
    let mut s=fixture();s.items[0].comments=2;s.items[0].body="Original description".into();
    s.items[0].related.push(data::Related{id:Some("2".into()),repo:"acme/sdk".into(),number:2,title:Some("Connected PR".into()),url:"https://github.com/acme/sdk/pull/2".into(),..Default::default()});
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:vec![data::Comment{id:42,author:"alice".into(),body:"A discussion reply".into(),..Default::default()}],next:Some(2),..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(1380.0,1200.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("comment-42").is_some());assert!(rt.find("more-comments").is_some());
    assert!(rt.find("related-acme/sdk-2").is_some());assert!(rt.find("open-related-acme/sdk-2").is_some());
    click(&mut rt,"related-acme/sdk-2");assert_eq!(rt.state.selected,"1");assert_eq!(rt.state.active_id(),"2");assert!(rt.find("comment-42").is_none());
    click(&mut rt,"back-to-original");assert_eq!(rt.state.active_id(),"1");assert!(rt.find("comment-42").is_some());
    rt.update(|s,_|s.select("1".into()));rt.frame(0.0);assert!(rt.find("comment-42").is_some());
}

#[test]
fn connected_preview_survives_refresh_outside_filters_and_keeps_notes_with_each_entry() {
    let mut s=fixture();s.repo_scope=RepoScope::only("acme/sdk");s.kind="issue".into();s.refilter();
    assert_eq!(s.selected,"1");assert_eq!(s.visible.len(),1);
    s.note=TextEdit::new("Original draft");s.note_state=NoteState::Dirty;
    s.preview_connection("outside-99".into());
    s.detail=Some(Item{id:"outside-99".into(),repo:"acme/other".into(),number:99,title:"A closed connection outside the list".into(),state:"closed".into(),advice:Advice{text:"Existing connected note".into(),..Default::default()},..Default::default()});
    s.restore_note();assert_eq!(s.note.text,"Existing connected note");
    s.note=TextEdit::new("Connected draft");s.note_state=NoteState::Dirty;
    // The same rebuild that follows a polling snapshot must not select the preview in the list.
    s.remember_note();s.rebuild();s.restore_note();
    assert_eq!(s.selected,"1");assert_eq!(s.active_id(),"outside-99");assert_eq!(s.visible.len(),1);
    assert_eq!(s.kind,"issue");assert_eq!(s.repo_scope,RepoScope::only("acme/sdk"));
    let mut rt=Runtime::new(s,ui::view,(1080.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("connection-preview").is_some());click(&mut rt,"back-to-original");
    assert_eq!(rt.state.selected,"1");assert_eq!(rt.state.active_id(),"1");assert_eq!(rt.state.note.text,"Original draft");
    assert_eq!(rt.state.drafts["outside-99"].0,"Connected draft");
    rt.update(|s,_|s.preview_connection("2".into()));rt.frame(0.0);
    click(&mut rt,"entry-1");assert!(rt.state.preview_id.is_none());assert_eq!(rt.state.note.text,"Original draft");
}

#[test]
fn long_notes_scroll_inside_a_bounded_editor_without_expanding_the_detail_page() {
    let mut s=fixture();s.note=TextEdit::new(&"A saved note with several lines of context.\n".repeat(300));
    s.note_state=NoteState::Dirty;s.items[0].body="The issue description stays below Notes.".into();
    let mut rt=Runtime::new(s,ui::view,(2400.0,1440.0),Env::new(true,1.0));rt.frame(0.0);
    let card=rt.elements().into_iter().find(|e|e.key.as_deref()==Some("advice-card")).unwrap();
    assert!(card.target[3]<=280.0,"Notes expanded to {} points",card.target[3]);
    let editor=rt.elements().into_iter().find(|e|e.key.as_deref()==Some("notes-scroll")).unwrap();
    assert_eq!(editor.target[3],160.0);
    let (x,y)=rt.find("notes-scroll").unwrap();rt.wheel(x,y,-600.0);rt.frame(0.1);
    let input=rt.elements().into_iter().find(|e|e.key.as_deref()==Some("advice-input")).unwrap();
    assert!(input.scrolled[1].abs()>0.0,"Long notes should scroll within the editor: {:?}",input.scrolled);
    assert!(rt.state.note.text.ends_with("context.\n"));
    rt.update(|s,_|s.note=TextEdit::new("Short note"));rt.frame(1.0);
    let card=rt.elements().into_iter().find(|e|e.key.as_deref()==Some("advice-card")).unwrap();
    assert!(card.target[3]<=280.0);
}

#[test]
fn select_all_in_notes_survives_background_refresh_and_replaces_the_whole_note() {
    let mut s=fixture();s.items[0].advice.text="A long saved note.\n".repeat(100);s.restore_note();
    let mut rt=Runtime::new(s,ui::view,(1380.0,1200.0),Env::new(true,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"notes-scroll");
    rt.key(KeyEvent{key:Key::Command('a'),mods:Mods{cmd:true,..Default::default()}});rt.frame(0.1);
    assert_eq!(rt.state.note.selected(),rt.state.note.text);
    // A polling refresh must not recreate the editor and clear its selection.
    rt.update(|s,_|{s.remember_note();s.rebuild();s.restore_note();});rt.frame(0.2);
    assert_eq!(rt.state.note.selected(),rt.state.note.text);
    rt.key(KeyEvent{key:Key::Text("Replacement note".into()),mods:Default::default()});rt.frame(0.3);
    assert_eq!(rt.state.note.text,"Replacement note");assert!(matches!(rt.state.note_state,NoteState::Dirty));
}

#[test]
fn settings_tabs_preserve_drafts_and_maintainers_support_add_remove_import_and_cancel(){
    let mut s=fixture();s.open_repository_preferences();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    rt.update(|s,_|{s.repo_prefs.add=TextEdit::new("acme/new-repo");s.repo_prefs.add();});rt.frame(0.0);
    click(&mut rt,"settings-maintainers");assert_eq!(rt.state.settings_tab,"maintainers");
    rt.update(|s,_|s.maint_prefs.add=TextEdit::new("@new-maintainer"));rt.frame(0.0);click(&mut rt,"add-maintainer");
    assert!(rt.state.maint_prefs.rows.iter().any(|m|m.login=="new-maintainer"));
    click(&mut rt,"settings-repositories");assert!(rt.state.repo_prefs.rows.iter().any(|r|r.name=="acme/new-repo"));
    click(&mut rt,"settings-maintainers");assert!(rt.state.maint_prefs.dirty);rt.update(|_,tx|tx.scroll_into_view("maintainer-new-maintainer"));rt.frame(1.0);click(&mut rt,"remove-maintainer-new-maintainer");
    assert!(!rt.state.maint_prefs.rows.iter().any(|m|m.login=="new-maintainer"));
    rt.update(|s,_|s.maint_prefs.import(vec![tracker_core::config::Maintainer{login:"imported-user".into(),github_id:"123".into()}]));rt.frame(0.0);
    assert_eq!(rt.state.maint_prefs.rows[0].login,"imported-user");assert!(rt.state.maint_prefs.dirty);
    click(&mut rt,"cancel-maintainer-settings");assert!(!rt.state.repo_prefs.open);
}

#[test]
fn maintainer_imports_accept_canonical_files_and_username_drafts_but_reject_bad_input(){
    let path=std::env::temp_dir().join(format!("tracker-maintainers-import-{}.json",std::process::id()));
    std::fs::write(&path,r#"{"maintainers":[{"githubId":"123","login":"Alice"},{"login":"@bob"}]}"#).unwrap();
    let imported=preferences::read_maintainer_import(path.clone()).unwrap();assert_eq!(imported.len(),2);assert_eq!(imported[1].login,"bob");
    std::fs::write(&path,r#"{"maintainers":[{"login":"alice"},{"login":"Alice"}]}"#).unwrap();assert!(preferences::read_maintainer_import(path.clone()).is_err());
    std::fs::write(&path,"bad json").unwrap();assert!(preferences::read_maintainer_import(path.clone()).is_err());let _=std::fs::remove_file(path);
}

#[test]
fn customer_settings_edit_metadata_and_cycle_card_styles() {
    use tracker_core::customer_config::{Customer,Highlight};
    let mut s=fixture();s.open_repository_preferences();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"settings-customers");
    rt.update(|s,_|s.cust_prefs.import(vec![Customer{login:"alice".into(),..Default::default()}]));rt.frame(0.0);
    click(&mut rt,"customer-name-0");rt.key(KeyEvent{key:Key::Text("Alice Example".into()),mods:Default::default()});rt.frame(0.0);
    click(&mut rt,"vip-notes-0");rt.key(KeyEvent{key:Key::Text("Acme".into()),mods:Default::default()});rt.frame(0.0);
    assert_eq!(rt.state.cust_prefs.customers()[0].full_name,"Alice Example");assert_eq!(rt.state.cust_prefs.customers()[0].notes,"Acme");
    click(&mut rt,"vip-company-0");rt.key(KeyEvent{key:Key::Text("Northstar".into()),mods:Default::default()});rt.frame(0.0);
    assert_eq!(rt.state.cust_prefs.customers()[0].company,"Northstar");assert_eq!(rt.state.cust_prefs.customers()[0].notes,"Acme");
    for style in Highlight::ALL {click(&mut rt,"highlight-customer-alice");rt.resolve_popup(Some(style.key())).unwrap();rt.frame(0.0);assert_eq!(rt.state.cust_prefs.rows[0].highlight,style);}
    click(&mut rt,"settings-repositories");click(&mut rt,"settings-customers");assert_eq!(rt.state.cust_prefs.customers()[0].notes,"Acme");
    click(&mut rt,"remove-customer-alice");assert!(rt.state.cust_prefs.customers().is_empty());
}

#[test]
fn fullscreen_shortcut_does_not_focus_search_and_works_in_settings() {
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));
    rt.on_key=Some(key);rt.frame(0.0);
    rt.key(KeyEvent{key:Key::Command('f'),mods:Mods{cmd:true,ctrl:true,..Default::default()}});rt.frame(0.0);
    rt.key(KeyEvent{key:Key::Text("x".into()),mods:Default::default()});rt.frame(0.0);
    assert!(rt.state.search.text.is_empty());
    rt.key(KeyEvent{key:Key::Command('f'),mods:Mods{cmd:true,..Default::default()}});rt.frame(0.0);
    rt.key(KeyEvent{key:Key::Text("x".into()),mods:Default::default()});rt.frame(0.0);
    assert_eq!(rt.state.search.text,"x");
    let mut state=fixture();state.open_repository_preferences();
    let mut tx=Tx::default();
    assert!(key(&mut state,&KeyEvent{key:Key::Command('f'),mods:Mods{cmd:true,ctrl:true,..Default::default()}},&mut tx));
    assert!(state.repo_prefs.open);
}

#[test]
fn repository_include_terms_are_editable_and_survive_tab_switching() {
    let mut rt=Runtime::new(fixture(),ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"manage-repositories");assert!(!rt.state.repo_prefs.is_dirty());
    click(&mut rt,"track-terms-acme/sdk");rt.key(KeyEvent{key:Key::Text("powersync, journeyapps".into()),mods:Default::default()});rt.frame(0.0);
    assert!(rt.state.repo_prefs.is_dirty());
    click(&mut rt,"settings-maintainers");click(&mut rt,"settings-repositories");
    let repos=rt.state.repo_prefs.repositories();assert_eq!(repos.iter().find(|r|r.name=="acme/sdk").unwrap().include_terms,vec!["powersync"," journeyapps"]);
}

#[test]
fn automatic_sync_status_is_visible_and_has_per_repository_details() {
    let mut s=fixture();s.data.backend.polling=true;
    s.data.repository_sync=serde_json::from_value(serde_json::json!([
        {"repo":"acme/sdk","status":"queued"},
        {"repo":"acme/core","status":"importing","stage":"backfill","backfill_page":3}
    ])).unwrap();
    assert_eq!(s.data.repository_sync[0].label(true),"Queued for initial import");
    assert!(s.data.repository_sync[1].label(true).contains("page 3"));
    assert!(s.data.repository_sync[0].label(false).contains("paused"));
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(rt.find("automatic-sync-status").is_some());click(&mut rt,"view-sync-settings");
    assert!(rt.find("repo-sync-acme/sdk").is_some());assert!(rt.find("repo-sync-acme/core").is_some());
}

#[test]
fn queued_history_is_distinct_from_an_active_import() {
    let mut sync:data::RepositorySync=serde_json::from_value(serde_json::json!({"repo":"tanstack/db","status":"waiting","stage":"backfill","backfill_page":8})).unwrap();
    assert_eq!(sync.label(true),"Waiting for next history page · page 8");
    sync.status="importing".into();assert_eq!(sync.label(true),"Importing history · page 8");
}

#[test]
fn unchanged_and_status_only_snapshots_preserve_cards_detail_and_editor() {
    let mut s=fixture();s.connection=Connection::Online;s.detail=Some(s.items[0].clone());
    s.note=TextEdit::new("Unsaved draft");s.note_state=NoteState::Dirty;
    let items=s.items.as_ptr();let visible=s.visible.as_ptr();let detail=s.detail.as_ref().unwrap().title.as_ptr();
    assert!(!s.apply_snapshot(s.data.clone()));
    let mut next=s.data.clone();next.repository_sync=serde_json::from_value(serde_json::json!([{"repo":"acme/sdk","status":"polling"}])).unwrap();
    assert!(s.apply_snapshot(next));
    assert_eq!(s.items.as_ptr(),items);assert_eq!(s.visible.as_ptr(),visible);assert_eq!(s.detail.as_ref().unwrap().title.as_ptr(),detail);
    assert_eq!(s.note.text,"Unsaved draft");assert!(matches!(s.note_state,NoteState::Dirty));assert_eq!(s.selected,"1");
    assert!(!s.apply_detail(s.detail.clone().unwrap()));
    s.note_state=NoteState::Saved;assert!(!s.apply_snapshot(s.data.clone()));assert!(matches!(s.note_state,NoteState::Saved));
}

#[test]
fn changed_entries_and_detail_update_without_overwriting_drafts() {
    let mut s=fixture();s.detail=Some(s.items[0].clone());s.note=TextEdit::new("Draft to preserve");s.note_state=NoteState::Dirty;
    let mut next=s.data.clone();next.pens[0].items[0].title="Updated title".into();
    assert!(s.apply_snapshot(next));assert_eq!(s.items[0].title,"Updated title");assert_eq!(s.selected,"1");
    let mut detail=s.items[0].clone();detail.body="Updated description".into();detail.comments=4;detail.advice.text="Saved elsewhere".into();
    assert!(s.apply_detail(detail.clone()));assert_eq!(s.current_item().unwrap().body,"Updated description");assert_eq!(s.note.text,"Draft to preserve");
    s.drafts.clear();s.note_state=NoteState::Clean;detail.advice.text="New saved advice".into();assert!(s.apply_detail(detail));assert_eq!(s.note.text,"New saved advice");
    let mut next=s.data.clone();next.pens[0].items.remove(0);assert!(s.apply_snapshot(next));assert_eq!(s.selected,"2");
}

#[test]
fn time_pen_changes_still_rebuild_the_filtered_list() {
    let mut s=fixture();s.pen="week".into();s.refilter();
    let mut next=s.data.clone();let item=next.pens[0].items.remove(0);next.pens[0].total-=1;
    next.pens.push(data::Pen{id:"month".into(),label:"Last month".into(),items:vec![item],total:1,..Default::default()});
    assert!(s.apply_snapshot(next));assert_eq!(s.visible.len(),2);assert_eq!(s.selected,"2");
}

#[test]
fn sync_status_changes_keep_list_position_and_note_selection_stable() {
    let mut s=fixture();s.connection=Connection::Online;s.data.backend.polling=true;
    s.detail=Some(s.items[0].clone());s.note=TextEdit::new("Saved note");s.note_state=NoteState::Clean;
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    click(&mut rt,"notes-scroll");rt.key(KeyEvent{key:Key::Command('a'),mods:Mods{cmd:true,..Default::default()}});rt.frame(0.1);
    let position=rt.find("entry-1").unwrap();let selected=rt.state.note.selected().to_owned();
    rt.update(|s,_|{let mut next=s.data.clone();next.repository_sync=serde_json::from_value(serde_json::json!([
        {"repo":"acme/sdk","status":"waiting","stage":"backfill","backfill_page":8},
        {"repo":"acme/core","status":"polling"}
    ])).unwrap();s.apply_snapshot(next);});rt.frame(0.2);
    assert_eq!(rt.find("entry-1").unwrap(),position);assert_eq!(rt.state.note.selected(),selected);
    rt.key(KeyEvent{key:Key::Text("Replacement".into()),mods:Default::default()});rt.frame(0.3);assert_eq!(rt.state.note.text,"Replacement");
}

#[test]
fn github_setup_blocks_board_settings_shortcuts_and_preserves_secure_input() {
    let mut s=fixture();s.auth=auth::Setup::default();
    let mut rt=Runtime::new(s,ui::view,(1080.0,800.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    assert!(rt.find("welcome-screen").is_some());assert!(rt.find("github-token-input").is_none());
    click(&mut rt,"get-started");rt.frame(0.0);
    assert!(rt.find("welcome-screen").is_none());
    assert!(rt.find("github-setup-dialog").is_some());assert!(rt.find("entry-1").is_none());assert!(rt.find("manage-repositories").is_none());
    rt.key(KeyEvent{key:Key::Command(','),mods:Mods{cmd:true,..Default::default()}});rt.frame(0.0);assert!(!rt.state.repo_prefs.open);
    rt.key(KeyEvent{key:Key::Command('r'),mods:Mods{cmd:true,..Default::default()}});rt.frame(0.0);assert!(!rt.state.refreshing);
    click(&mut rt,"github-token-input");rt.key(KeyEvent{key:Key::Text("fake-token-for-ui-test".into()),mods:Default::default()});rt.frame(0.0);
    assert!(rt.state.auth.token.is_secure());assert_eq!(rt.state.auth.token.text,"fake-token-for-ui-test");
    rt.key(KeyEvent{key:Key::Escape,mods:Default::default()});rt.frame(0.0);assert!(rt.find("github-setup-dialog").is_some());
    // Successful validation unlocks the board and clears the secret field. Preview mode avoids real credential storage.
    rt.update(|s,_|s.connected("fake-test-token".into()));rt.frame(0.0);
    assert!(rt.find("github-setup-dialog").is_none());assert!(rt.find("entry-1").is_some());assert!(rt.state.auth.token.text.is_empty());
}

#[test]
fn github_setup_empty_token_and_failed_validation_keep_gate_closed() {
    let mut s=fixture();s.auth=auth::Setup::default();s.auth.connect();
    assert!(s.auth_blocked());assert!(s.auth.error.is_some());assert!(!s.auth.saving);
    s.auth.saving=true;s.auth.connected.request((),|_|Err("Invalid token".into()));
    let deadline=std::time::Instant::now()+Duration::from_secs(2);
    while s.auth.saving && std::time::Instant::now()<deadline {poll(&mut s,0.0,&mut Tx::default());std::thread::sleep(Duration::from_millis(5));}
    assert!(s.auth_blocked());assert!(!s.auth.saving);assert_eq!(s.auth.error.as_deref(),Some("Invalid token"));
}

#[test]
fn expired_token_reconnect_is_dismissible_and_preserves_cached_work() {
    let mut s=fixture();s.note=TextEdit::new("Unsaved local note");
    let mut expired=s.data.clone();expired.backend.auth_failed=true;expired.backend.polling=false;
    s.apply_snapshot(expired.clone());
    assert!(s.auth.reconnect_open);assert!(!s.auth_blocked());
    let mut rt=Runtime::new(s,ui::view,(1080.0,800.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    assert!(rt.find("github-token-input").is_some());
    click(&mut rt,"quit-github-setup");rt.frame(0.0);
    assert!(rt.find("entry-1").is_some());assert!(rt.find("reconnect-github").is_some());
    assert_eq!(rt.state.note.text,"Unsaved local note");
    rt.state.apply_snapshot(expired);rt.frame(0.0);assert!(!rt.state.auth.reconnect_open);
    click(&mut rt,"reconnect-github");rt.frame(1.0);
    assert!(rt.state.auth.reconnect_open);
    rt.key(KeyEvent{key:Key::Escape,mods:Default::default()});rt.frame(0.0);
    assert!(!rt.state.auth.reconnect_open);assert!(rt.state.auth.disconnected);
    rt.update(|s,tx|{s.connected("replacement-test-token".into());tx.instant();});rt.frame(0.0);
    assert!(!rt.state.auth.disconnected);assert!(rt.find("reconnect-github").is_none());
    assert_eq!(rt.state.note.text,"Unsaved local note");
}

#[test]
fn developer_queues_ignore_followup_scope_and_preserve_notes_and_order(){
    let mut s=fixture();s.repo_scope=RepoScope::only("acme/nothing");s.pen="day".into();s.newest=true;
    let mut item=s.items[0].clone();item.developer_reasons=vec!["review".into(),"author".into(),"mentioned".into()];item.action_times.insert("review".into(),"2026-11-11T00:00:00Z".into());
    let mut second=s.items[1].clone();second.developer_reasons=vec!["review".into(),"assigned".into()];second.action_times.insert("review".into(),"2026-11-01T00:00:00Z".into());
    s.dev.items=vec![item,second];s.dev.profiles=vec!["Chriztiaan".into(),"alice".into()];
    s.developer_queue("review");assert_eq!(s.visible.len(),2);assert_eq!(s.items[s.visible[0]].id,"2");assert!(!s.newest);
    s.select("2".into());s.note=TextEdit::new("Draft survives navigation");s.note_state=NoteState::Dirty;s.select("1".into());s.select("2".into());assert_eq!(s.note.text,"Draft survives navigation");
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"order");assert_eq!(rt.state.items[rt.state.visible[0]].id,"1");
    click(&mut rt,"dev-participating");assert_eq!(rt.state.visible.len(),1);assert!(rt.state.newest);
    click(&mut rt,"nav-all");assert!(!rt.state.dev.active);assert!(rt.state.newest);assert!(rt.state.visible.is_empty());
    assert_eq!(rt.state.drafts["2"].0,"Draft survives navigation");
}

#[test]
fn developer_profile_picker_and_history_do_not_mix_profiles(){
    let mut s=fixture();s.dev.profiles=vec!["Chriztiaan".into(),"alice".into()];s.dev.items=s.items.clone();s.dev.items[0].developer_reasons=vec!["author".into()];s.dev.items[0].state="closed".into();
    s.developer_queue("created");assert!(s.visible.is_empty());s.dev.closed=true;s.rebuild();assert_eq!(s.visible.len(),1);
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
    click(&mut rt,"developer-profile");rt.frame(1.0);click(&mut rt,"developer-select-alice");assert_eq!(rt.state.dev.login,"alice");assert!(rt.state.items.is_empty());assert!(!rt.state.dev.picker);
}

#[test]
fn developer_profile_selection_roundtrips_and_discussion_highlights_are_identity_specific(){
 let path=std::env::temp_dir().join(format!("developer-profile-{}.json",std::process::id()));
 developer::save_preference(&path,"Chriztiaan").unwrap();assert_eq!(developer::load_preference(&path).as_deref(),Some("Chriztiaan"));
 developer::save_preference(&path,"alice").unwrap();assert_eq!(developer::load_preference(&path).as_deref(),Some("alice"));std::fs::remove_file(path).unwrap();
 let mut s=fixture();developer::preview(&mut s);
 let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.frame(0.0);
 assert!(rt.find("selected-developer-review-2").is_some());assert!(rt.find("selected-developer-comment-1").is_none());
 rt.update(|s,tx|{s.dev.login="alice".into();tx.instant();});rt.frame(0.0);
 assert!(rt.find("selected-developer-review-2").is_none());assert!(rt.find("selected-developer-comment-1").is_some());
}

#[test]
fn github_actions_target_the_card_without_changing_selection() {
    let mut s=fixture();
    for item in &mut s.items {item.url=format!("https://github.com/acme/sdk/issues/{}",item.number);}
    s.select("1".into());
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(true,1.0));rt.frame(0.0);
    click(&mut rt,"entry-open-2");
    assert_eq!(rt.take_url_requests(),["https://github.com/acme/sdk/issues/2"]);
    assert_eq!(rt.state.selected,"1","nested action must not select the card");
    click(&mut rt,"detail-title-link");
    assert_eq!(rt.take_url_requests(),["https://github.com/acme/sdk/issues/1"]);
    let (x,y)=rt.find("entry-3").unwrap();rt.secondary_down(x,y);rt.secondary_up(x,y);rt.frame(0.0);
    assert!(rt.popup_menu().is_some());
    rt.resolve_popup(Some("open-github")).unwrap();rt.frame(0.0);
    assert_eq!(rt.take_url_requests(),["https://github.com/acme/sdk/issues/3"]);
    assert_eq!(rt.state.selected,"1");
}

#[test]
fn comment_preview_and_full_collapse_are_independent() {
    let mut s=fixture();s.show_advice=false;
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:vec![
        data::Comment{id:42,kind:"comment".into(),author:"alice".into(),body:"First line\nSecond line\nThird line\nFourth line\nFifth line\nSixth line\nSeventh line".into(),..Default::default()},
        data::Comment{id:42,kind:"review".into(),author:"bob".into(),body:"Looks good.".into(),..Default::default()},
    ],..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(1380.0,1500.0),Env::new(true,1.0));rt.frame(0.0);rt.frame(0.1);
    assert!(rt.find("more-comment-comment-42").is_some());
    assert!(rt.find("more-comment-review-42").is_none());
    click(&mut rt,"more-comment-comment-42");assert!(rt.state.comment_display["1:comment:42"].expanded);
    click(&mut rt,"collapse-comment-comment-42");assert!(rt.state.comment_display["1:comment:42"].collapsed);
    assert!(rt.find("comment-body-comment-42").is_none());assert!(rt.find("more-comment-comment-42").is_none());assert!(rt.find("comment-42").is_some());
    click(&mut rt,"collapse-comment-comment-42");assert!(rt.state.comment_display["1:comment:42"].expanded);assert!(rt.find("comment-body-comment-42").is_some());
    click(&mut rt,"more-comment-comment-42");assert!(!rt.state.comment_display["1:comment:42"].expanded);
    click(&mut rt,"collapse-comment-review-42");assert!(rt.state.comment_display["1:review:42"].collapsed);assert!(!rt.state.comment_display["1:comment:42"].collapsed);
}

#[test]
fn comment_preview_remeasures_wrapped_paragraphs_when_window_resizes() {
    let mut s=fixture();s.show_advice=false;
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:vec![data::Comment{id:8,kind:"comment".into(),body:"A paragraph with several words that wraps as the detail pane gets narrower. ".repeat(7),..Default::default()}],..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(2200.0,1500.0),Env::new(true,1.0));rt.frame(0.0);rt.frame(0.1);
    assert!(rt.find("more-comment-comment-8").is_none());
    rt.set_viewport((1100.0,1500.0),1.0);rt.frame(0.2);rt.frame(0.3);
    assert!(rt.find("more-comment-comment-8").is_some());
    rt.set_viewport((2200.0,1500.0),1.0);rt.frame(0.4);rt.frame(0.5);
    assert!(rt.find("more-comment-comment-8").is_none());
}

#[test]
fn empty_reviews_explain_missing_summary_and_can_collapse() {
    let mut s=fixture();s.show_advice=false;
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:vec![data::Comment{id:9,kind:"review".into(),body:"  \n".into(),review_state:"COMMENTED".into(),..Default::default()}],..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(1380.0,1200.0),Env::new(true,1.0));rt.frame(0.0);
    assert!(rt.find("comment-body-review-9").is_some());assert!(rt.find("more-comment-review-9").is_none());
    click(&mut rt,"collapse-comment-review-9");assert!(rt.find("comment-body-review-9").is_none());
    click(&mut rt,"collapse-comment-review-9");assert!(rt.find("comment-body-review-9").is_some());
}

#[test]
fn collapsing_a_scrolled_comment_keeps_its_header_in_place() {
    let mut s=fixture();s.show_advice=false;
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:(0..20).map(|i|data::Comment{id:i,kind:"comment".into(),body:"One line of discussion.\n".repeat(14),..Default::default()}).collect(),..Default::default()});
    s.comment_display.entry("1:comment:0".into()).or_default().expanded=true;
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(true,1.0));rt.frame(0.0);rt.frame(0.1);
    rt.update(|_,tx|{tx.scroll_key_to("comment-header-1:comment:0",40.0);tx.instant();});rt.frame(0.2);
    let before=rt.find("collapse-comment-comment-0").unwrap().1;
    click(&mut rt,"collapse-comment-comment-0");rt.frame(0.3);
    let after=rt.find("collapse-comment-comment-0").unwrap().1;
    assert!((before-after).abs()<1.0,"header moved from {before} to {after}");
    click(&mut rt,"collapse-comment-comment-0");rt.frame(0.4);
    assert!((before-rt.find("collapse-comment-comment-0").unwrap().1).abs()<1.0);
}

#[test]
fn author_profiles_open_without_selecting_cards_and_vip_menu_preserves_drafts() {
    let mut s=fixture();s.select("2".into());s.show_advice=false;
    s.cust_prefs.load(tracker_core::customer_config::CustomerSettings{customers:vec![tracker_core::customer_config::Customer{login:"alice".into(),notes:"Unsaved context".into(),..Default::default()}],..Default::default()});s.cust_prefs.dirty=true;
    let mut rt=Runtime::new(s,ui::view,(1380.0,1200.0),Env::new(true,1.0));rt.frame(0.0);
    click(&mut rt,"card-profile-1");assert_eq!(rt.take_url_requests(),["https://github.com/alice"]);assert_eq!(rt.state.selected,"2");
    click(&mut rt,"detail-author-profile");assert_eq!(rt.take_url_requests(),["https://github.com/bob"]);
    let (x,y)=rt.find("card-profile-1").unwrap();rt.secondary_down(x,y);rt.secondary_up(x,y);rt.frame(0.0);
    let expected=rune::PopupMenu::new().item(rune::PopupItem::new("profile","Open GitHub profile")).separator().item(rune::PopupItem::new("vip","Edit VIP…"));assert_eq!(rt.popup_menu(),Some(&expected));
    rt.resolve_popup(Some("vip")).unwrap();rt.frame(0.0);
    assert_eq!(rt.state.settings_tab,"customers");assert_eq!(rt.state.cust_prefs.rows.len(),1);assert_eq!(rt.state.cust_prefs.notes[0].text,"Unsaved context");assert!(rt.state.cust_prefs.dirty);assert_eq!(rt.state.selected,"2");
}

#[test]
fn commenter_menu_prefills_new_vip_without_saving_and_unknown_authors_are_not_links() {
    let mut s=fixture();s.show_advice=false;
    s.discussions.insert("1".into(),DiscussionState{loaded:true,comments:vec![data::Comment{id:4,kind:"comment".into(),author:"carol".into(),body:"Hello".into(),..Default::default()}],..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(1380.0,1200.0),Env::new(true,1.0));rt.frame(0.0);
    click(&mut rt,"comment-profile-comment-4");assert_eq!(rt.take_url_requests(),["https://github.com/carol"]);
    let (x,y)=rt.find("comment-profile-comment-4").unwrap();rt.secondary_down(x,y);rt.secondary_up(x,y);rt.frame(0.0);rt.resolve_popup(Some("vip")).unwrap();rt.frame(0.0);
    assert_eq!(rt.state.cust_prefs.rows[0].login,"carol");assert!(rt.state.cust_prefs.dirty);assert!(!rt.state.cust_prefs.saving);assert_eq!(rt.state.selected,"1");
    rt.update(|s,_|{s.repo_prefs.open=false;s.items[0].author=None;});rt.frame(0.0);assert!(rt.find("card-profile-1").is_none());
}

#[test]
fn saved_vips_update_all_visible_copies_without_closing_settings() {
    let mut s=State::init();s.preview=true;
    crate::showcase::work(&mut s);
    let selected=s.selected.clone();
    let settings=tracker_core::customer_config::CustomerSettings{customers:vec![tracker_core::customer_config::Customer{login:"morgan".into(),full_name:"Morgan Lee".into(),company:"Northstar".into(),notes:"Call Friday".into(),highlight:tracker_core::customer_config::Highlight::SubtleTint,..Default::default()}],..Default::default()};
    s.apply_vips(settings);
    for item in s.items.iter().chain(s.dev.items.iter()).chain(s.data.pens.iter().flat_map(|p|p.items.iter())).chain(s.detail.iter()) {
        if let Some(a)=item.author.as_ref().filter(|a|a.login=="morgan") {assert!(a.customer);assert_eq!(a.highlight,"subtle-tint");assert_eq!(a.full_name,"Morgan Lee");assert_eq!(a.company,"Northstar");assert_eq!(a.notes,"Call Friday");}
    }
    assert_eq!(s.selected,selected);
    s.apply_vips(Default::default());
    let a=s.detail.as_ref().unwrap().author.as_ref().unwrap();
    assert!(!a.customer);assert!(a.notes.is_empty());assert!(a.company.is_empty());
}

#[test]
fn settings_close_guards_changes_across_tabs_and_cancel_discards() {
    let mut s=fixture();s.open_repository_preferences();s.cust_prefs.dirty=true;
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.frame(0.0);
    rt.key(KeyEvent{key:Key::Escape,mods:Default::default()});rt.frame(0.0);
    assert!(rt.find("discard-settings-dialog").is_some());assert!(rt.state.repo_prefs.open);
    click(&mut rt,"keep-editing-settings");assert!(rt.state.cust_prefs.dirty);assert!(!rt.state.settings_close_confirm);
    rt.key(KeyEvent{key:Key::Command('w'),mods:Default::default()});rt.frame(0.0);
    click(&mut rt,"discard-settings");assert!(!rt.state.repo_prefs.open);assert!(!rt.state.settings_dirty());
    rt.update(|s,_|s.open_repository_preferences());rt.frame(0.0);
    click(&mut rt,"cancel-repository-preferences");assert!(!rt.state.repo_prefs.open);assert!(!rt.state.settings_close_confirm);
}

#[test]
fn vip_focus_and_save_do_not_leave_settings_dirty() {
    let mut s=fixture();s.settings_window=true;s.repo_prefs.open=true;s.settings_tab="customers".into();
    s.cust_prefs.load(tracker_core::customer_config::CustomerSettings{customers:vec![tracker_core::customer_config::Customer{login:"alice".into(),full_name:"Alice".into(),..Default::default()}],..Default::default()});
    let mut rt=Runtime::new(s,ui::view,(960.0,720.0),Env::new(false,1.0));rt.frame(0.0);
    assert!(!rt.state.settings_dirty());
    click(&mut rt,"customer-name-0");assert!(!rt.state.settings_dirty(),"focusing a field must not mark it edited");
    rt.key(KeyEvent{key:Key::Text("X".into()),mods:Default::default()});rt.frame(0.0);assert!(rt.state.settings_dirty());
    rt.update(|s,_|{let settings=tracker_core::customer_config::CustomerSettings{customers:s.cust_prefs.customers(),..Default::default()};s.apply_vips(settings);});rt.frame(0.0);
    assert!(!rt.state.settings_dirty(),"saved fields must stay clean while focused");
}

#[test]
fn mock_token_runs_onboarding_and_all_demo_queues_without_backend() {
    let mut s=fixture();s.auth=auth::Setup::default();s.data=Default::default();s.items.clear();
    let mut rt=Runtime::new(s,ui::view,(1380.0,880.0),Env::new(false,1.0));rt.on_key=Some(key);rt.poll=Some(poll);rt.frame(0.0);
    click(&mut rt,"get-started");click(&mut rt,"github-token-input");
    rt.key(KeyEvent{key:Key::Text("mock_token".into()),mods:Default::default()});rt.frame(0.0);
    click(&mut rt,"connect-github-token");
    assert!(rt.state.demo&&rt.state.preview&&rt.state.auth.ready);assert!(!rt.state.data.backend.polling);assert!(rt.state.auth.token.text.is_empty());
    assert_eq!(rt.state.age("2026-11-17T09:00:00Z"),"3h");
    for pen in ["day","week","month","six-months","cold"] {click(&mut rt,&format!("nav-{pen}"));assert!(!rt.state.visible.is_empty(),"{pen}");}
    for queue in ["review","assigned","created","participating"] {click(&mut rt,&format!("dev-{queue}"));assert!(!rt.state.visible.is_empty(),"{queue}");}
    rt.frame(3600.0);assert!(!rt.state.dev.loading);assert!(!rt.state.refreshing);
    click(&mut rt,"manage-repositories");click(&mut rt,"settings-maintainers");assert_eq!(rt.state.maint_prefs.rows[0].login,"alex");
    click(&mut rt,"settings-customers");assert_eq!(rt.state.cust_prefs.rows[0].login,"morgan");
    click(&mut rt,"highlight-customer-morgan");rt.resolve_popup(Some("subtle-tint")).unwrap();rt.frame(0.0);
    click(&mut rt,"save-customer-settings");assert!(!rt.state.settings_dirty());assert_eq!(rt.state.cust_prefs.rows[0].highlight.key(),"subtle-tint");
    click(&mut rt,"cancel-customer-settings");click(&mut rt,"manage-repositories");click(&mut rt,"settings-customers");
    assert_eq!(rt.state.cust_prefs.rows[0].highlight.key(),"subtle-tint");
    rt.update(|s,_|s.connected("mock_token".into()));rt.frame(0.0);
    assert_eq!(rt.state.cust_prefs.rows[0].highlight.key(),"gold-star");
}

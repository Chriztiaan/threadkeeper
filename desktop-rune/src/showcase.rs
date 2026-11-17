//! Fictional, deterministic presentation data. Never contacts GitHub or writes user data.
use crate::{State, data, developer, Connection};
use serde_json::json;
pub fn followup(s:&mut State) {
    let titles=["Offline edits disappear after reconnecting", "Add keyboard navigation to the command palette", "Document the migration from v2", "Keep scroll position when new messages arrive", "Support custom retry intervals", "Improve contrast in light mode", "Make plugin errors easier to diagnose", "Add a React starter example"];
    let repos=["acme/atlas","acme/atlas","community/atlas-plugin","acme/atlas","acme/client","acme/atlas","community/atlas-plugin","acme/client"];
    let authors=["morgan","sam","riley","jamie","taylor","casey","robin","drew"];
    let mut items=vec![];
    for (n,title) in titles.iter().enumerate() {
        let day=16-n as u32;
        let at=format!("2026-11-{day:02}T09:00:00Z");
        items.push(json!({"id":format!("showcase-{n}"),"repo":repos[n],"number":214+n,"type":if n==1||n==4{"pr"}else{"issue"},"state":"open","title":title,"author":{"login":authors[n],"vip":n==0,"fullName":"Morgan Lee","company":"Northstar","notes":"Partner developer. Follow up before the next release.","highlight":"gold-star"},"comments":3+n,"created_at":at,"updated_at":at,"body":"## What happened\n\nEdits made while offline should remain available when the connection returns. A VIP shared a reproducible example.\n\n## Next step\n\nConfirm the fix with their example and follow up before the next release.","labels":[{"name":if n==0{"bug"}else{"enhancement"},"color":if n==0{"cf6d64"}else{"6d9b91"}}],"engagement":{"at":at,"kind":"comment","author":{"login":authors[n]},"pending":false},"advice":{"text":if n==0{"Reproduce with Morgan’s example. Share the fix and confirm it resolves the issue."}else{""},"enabled":true},"url":"https://github.com/example/example"}));
    }
    s.data=serde_json::from_value(json!({"repositories":["acme/atlas","acme/client","community/atlas-plugin"],"maintainerCount":4,"repositorySync":[{"repo":"acme/atlas","status":"ready"},{"repo":"acme/client","status":"ready"},{"repo":"community/atlas-plugin","status":"ready"}],"total":8,"backend":{"message":"Demo workspace","polling":true},"pens":[{"id":"day","label":"Last 24 hours","items":items[..2],"total":2},{"id":"week","label":"Last week","items":items[2..5],"total":3},{"id":"month","label":"Last month","items":items[5..],"total":3},{"id":"months","label":"Last 6 months","items":[],"total":0},{"id":"stale","label":"Cold & stale","items":[],"total":0}]})).unwrap();
    s.dev.active=false;s.dev.login="alex".into();s.dev.profiles=vec!["alex".into(),"sam".into()];
    s.items=s.data.items();s.dev.items=s.items[..3].to_vec();
    for (i,item) in s.dev.items.iter_mut().enumerate(){item.developer_reasons=vec![if i==0{"commented"}else if i==1{"review"}else{"author"}.into()];}
    s.dev.loaded_queues=developer::QUEUES.iter().map(|q|q.0.into()).collect();
    s.repo_scope=Default::default();s.pen.clear();s.kind.clear();s.search=rune::TextEdit::new("");s.connection=Connection::Online;s.newest=true;s.appearance=Some(true);s.show_advice=true;s.rebuild();s.select("showcase-0".into());s.detail=s.items.first().cloned();s.restore_note();
}
pub fn work(s:&mut State) {
    followup(s);
    s.dev.items=s.items[..5].to_vec();
    for (i,item) in s.dev.items.iter_mut().enumerate(){
        item.developer_reasons=if i==1{vec!["review".into(),"assigned".into()]}else if i==4{vec!["author".into()]}else{vec!["commented".into(),"mentioned".into()]};
    }
    s.developer_queue("participating");s.select("showcase-0".into());s.detail=s.dev.items.first().cloned();s.dev.checked=Some(1794819600);s.show_advice=false;
    s.discussions.insert("showcase-0".into(),crate::DiscussionState{loaded:true,comments:vec![
        data::Comment{id:1,kind:"comment".into(),author:"morgan".into(),body:"I’ve added a small reproduction. The edits are present until the connection comes back.".into(),created_at:"2026-11-16T08:00:00Z".into(),..Default::default()},
        data::Comment{id:2,kind:"comment".into(),author:"alex".into(),body:"Thanks, Morgan. I can reproduce it. I’ll test the reconnect fix against your example and post an update here.".into(),created_at:"2026-11-16T09:00:00Z".into(),..Default::default()},
    ],..Default::default()});
}

pub fn vips(s:&mut State) {
    followup(s);s.settings_window=true;s.repo_prefs.open=true;s.settings_page("customers");
    s.cust_prefs.load(tracker_core::customer_config::CustomerSettings{customers:vec![tracker_core::customer_config::Customer{github_id:"7".into(),login:"morgan".into(),full_name:"Morgan Lee".into(),company:"Northstar".into(),notes:"Partner developer.\nFollow up before the next release.".into(),..Default::default()}],path:"vips.json".into(),..Default::default()});
}

pub fn comments(s:&mut State) {
    work(s);
    if let Some(item)=s.detail.as_mut(){item.body="Tracking the reconnect fix and follow-up.".into();if let Some(author)=item.author.as_mut(){author.notes.clear();}}
    if let Some(d)=s.discussions.get_mut("showcase-0") {d.comments[1].body="Thanks, Morgan. I can reproduce the issue.\nHere is the plan for the fix:\n1. Keep pending edits in the local queue.\n2. Reconnect without discarding that queue.\n3. Apply acknowledgements in order.\n4. Retry interrupted uploads safely.\n5. Add coverage for connection drops.\nI will share a build for you to try tomorrow.".into();}
}

pub struct SavedSettings {
    pub repositories:Vec<tracker_core::config::Repository>,
    pub maintainers:Vec<tracker_core::config::Maintainer>,
    pub vips:Vec<tracker_core::customer_config::Customer>,
}
pub fn repositories(s:&mut State) {
    if let Some(saved)=&s.demo_saved{s.repo_prefs.load(tracker_core::repository_config::RepositorySettings{repositories:saved.repositories.clone(),path:"Demo · in memory".into(),..Default::default()});return;}
    s.repo_prefs.load(tracker_core::repository_config::RepositorySettings{repositories:s.data.repositories.iter().map(|name|tracker_core::config::Repository{name:name.clone(),include_terms:if name.starts_with("community/"){vec!["atlas".into()]}else{vec![]},enabled:true}).collect(),path:"Demo · in memory".into(),..Default::default()});
}
pub fn maintainers(s:&mut State) {
    if let Some(saved)=&s.demo_saved{s.maint_prefs.load(tracker_core::maintainer_config::MaintainerSettings{maintainers:saved.maintainers.clone(),path:"Demo · in memory".into(),..Default::default()});return;}
    s.maint_prefs.load(tracker_core::maintainer_config::MaintainerSettings{maintainers:["alex","sam"].iter().map(|login|tracker_core::config::Maintainer{login:(*login).into(),github_id:String::new()}).collect(),path:"Demo · in memory".into(),..Default::default()});
}
pub fn vip_roster(s:&mut State) {
    if let Some(saved)=&s.demo_saved{s.apply_vips(tracker_core::customer_config::CustomerSettings{customers:saved.vips.clone(),path:"Demo · in memory".into(),..Default::default()});return;}
    s.apply_vips(tracker_core::customer_config::CustomerSettings{customers:vec![tracker_core::customer_config::Customer{github_id:"7001".into(),login:"morgan".into(),full_name:"Morgan Lee".into(),company:"Northstar".into(),notes:"Partner developer.\nFollow up before the next release.".into(),..Default::default()}],path:"Demo · in memory".into(),..Default::default()});
}
pub fn demo(s:&mut State) {
    s.demo_saved=None;
    comments(s);
    let mut items=s.data.items();
    let dates=["2026-11-17T09:00:00Z","2026-11-15T09:00:00Z","2026-11-12T09:00:00Z","2026-10-29T09:00:00Z","2026-09-10T09:00:00Z","2026-06-12T09:00:00Z","2026-03-10T09:00:00Z","2025-12-01T09:00:00Z"];
    for (i,item) in items.iter_mut().enumerate(){item.created_at=dates[i].into();item.updated_at=dates[i].into();item.engagement.at=dates[i].into();if i==0{item.comments=2;}if i==4{if let Some(author)=item.author.as_mut(){author.login="alex".into();}}}
    s.data.pens=[("day","Last 24 hours",0,1),("week","Last week",1,3),("month","Last month",3,4),("six-months","Last 6 months",4,6),("cold","Cold & stale",6,8)].into_iter().map(|(id,label,start,end)|data::Pen{id:id.into(),label:label.into(),total:end-start,items:items[start..end].to_vec(),has_more:false}).collect();
    s.data.maintainers=2;
    s.data.backend.message="Demo · fictional data · polling off".into();s.data.backend.polling=false;
    s.dev.items=items[..5].to_vec();
    for (i,item) in s.dev.items.iter_mut().enumerate(){item.developer_reasons=match i{1=>vec!["review".into()],2=>vec!["assigned".into()],4=>vec!["author".into()],_=>vec!["commented".into(),"mentioned".into()]};}
    if let Some(a)=s.dev.items[4].author.as_mut(){a.login="alex".into();}
    s.dev.profiles=vec!["alex".into()];s.dev.login="alex".into();
    s.detail=None;s.followup("");s.select("showcase-0".into());s.show_advice=false;
    repositories(s);maintainers(s);vip_roster(s);
    s.demo_saved=Some(SavedSettings{repositories:s.repo_prefs.repositories(),maintainers:s.maint_prefs.rows.clone(),vips:s.cust_prefs.customers()});
}

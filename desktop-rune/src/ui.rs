use crate::{developer,Connection, NoteState, State, data::{self}};
use rune::{col, el, row, text, text_input, icon, Color, Cx, El, Family, GrowthSpec, Icon, ScrollSpec, TextStyle};

static THREADKEEPER: Icon = Icon { name: "threadkeeper-brand", view: 1024.0, body: include_str!("../assets/threadkeeper-mark-body.svg") };
static INBOX: Icon = Icon { name: "inbox", view: 24.0, body: r#"<path d="M4 4h16l2 10v6H2v-6L4 4z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="M2 14h6l2 3h4l2-3h6" fill="none" stroke="currentColor" stroke-width="1.7"/>"# };
static ISSUE: Icon = Icon { name: "issue", view: 24.0, body: r#"<circle cx="12" cy="12" r="8.3" fill="none" stroke="currentColor" stroke-width="1.8"/><circle cx="12" cy="12" r="2"/>"# };
static COMMENT: Icon = Icon { name: "comment", view: 24.0, body: r#"<path d="M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H9l-6 4V6a2 2 0 0 1 2-2Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"# };
static PR: Icon = Icon { name: "pull-request", view: 24.0, body: r#"<circle cx="6" cy="5" r="2.5" fill="none" stroke="currentColor" stroke-width="1.7"/><circle cx="6" cy="19" r="2.5" fill="none" stroke="currentColor" stroke-width="1.7"/><circle cx="18" cy="19" r="2.5" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="M6 8v8M18 16V9c0-3-3-4-6-4m2-3-3 3 3 3" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"# };
static SEARCH: Icon = Icon { name: "search", view: 24.0, body: r#"<circle cx="10.5" cy="10.5" r="6.5" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m16 16 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"# };
static REFRESH: Icon = Icon { name: "refresh", view: 24.0, body: r#"<path d="M20 7v5h-5M4 17v-5h5M5 8a8 8 0 0 1 14-2l1 2M4 16l1 2a8 8 0 0 0 14-2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"# };
static SUN: Icon = Icon { name: "appearance", view: 24.0, body: r#"<circle cx="12" cy="12" r="4" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="M12 1v3m0 16v3M1 12h3m16 0h3M4 4l2 2m12 12 2 2M4 20l2-2M18 6l2-2" fill="none" stroke="currentColor" stroke-width="1.7"/>"# };
static EXTERNAL: Icon = Icon { name: "open-external", view: 24.0, body: r#"<path d="M14 4h6v6m0-6L10 14M10 5H5v14h14v-5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"# };
static CLOCK: Icon = Icon { name: "clock", view: 24.0, body: r#"<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="M12 7v5l3 2" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>"# };
static FOLDER: Icon = Icon { name: "repository", view: 24.0, body: r#"<path d="M3 6h7l2 2h9v12H3V6z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/>"# };
static NOTE: Icon = Icon { name: "note", view: 24.0, body: r#"<path d="M5 3h14v12l-6 6H5V3Z M13 21v-6h6 M8 7h8 M8 11h8" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/>"# };
static CHEVRON: Icon = Icon { name: "chevron", view: 24.0, body: r#"<path d="m9 5 7 7-7 7" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"# };

#[derive(Clone, Copy)]
pub struct Paint { dark: bool, bg: Color, side: Color, ink: Color, secondary: Color, muted: Color, line: Color, card: Color, selected: Color, hover: Color, accent: Color, green: Color, purple: Color }
fn c(hex: &str) -> Color { rune::color::hex(hex).unwrap_or(Color::BLACK) }
fn paint(dark: bool) -> Paint {
    if dark { Paint { dark, bg:c("#1d1e22"), side:c("#25262c"), ink:c("#eeeeF2"), secondary:c("#b1b2bd"), muted:c("#777985"), line:c("#35363e"), card:c("#27282e"), selected:c("#493a25"), hover:c("#2c2d35"), accent:c("#f2b85b"), green:c("#6ec69c"), purple:c("#ad98ee") } }
    else { Paint { dark, bg:c("#fcfcfe"), side:c("#f0f1f5"), ink:c("#242631"), secondary:c("#555b68"), muted:c("#707582"), line:c("#d9dce5"), card:c("#f0f1f6"), selected:c("#f7ead3"), hover:c("#eaeaf2"), accent:c("#895510"), green:c("#287b53"), purple:c("#7650ae") } }
}
fn label(value: impl Into<String>, size: f32, weight: f32, color: Color) -> El { text(value).font(TextStyle::new(size,weight).lh(size*1.42).tracking(if size<10.0 {0.5}else{0.0}).family(Family::Named(".SF NS"))).color(color) }
fn rule(p: Paint) -> El { el().h(0.5).w_full().fill(p.line).a11y_hidden() }
fn pill(value: impl Into<String>, color: Color) -> El { row().px(7.0).py(3.0).radius(5.0).fill(color.alpha(0.10)).child(label(value,10.5,500.0,color)) }
fn label_badge(value: &data::Label, p: Paint) -> El {
    let hex=value.color.as_deref().unwrap_or("").trim_start_matches('#');
    let rgb=(hex.len()==6&&hex.bytes().all(|b|b.is_ascii_hexdigit())).then(||u32::from_str_radix(hex,16).ok()).flatten();
    let (bg,ink)=if let Some(rgb)=rgb {
        let linear=|shift: u32| {let v=((rgb>>shift)&255u32) as f32/255.0;if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)}};
        let luminance=0.2126*linear(16)+0.7152*linear(8)+0.0722*linear(0);
        (c(&format!("#{hex}")),if luminance>0.179 {c("#111111")}else{Color::WHITE})
    }else{(p.card,p.secondary)};
    row().max_w(340.0).min_w(0.0).px(6.0).py(3.0).radius(5.0).fill(bg).label(&value.name)
        .child(label(&value.name,10.0,500.0,ink).truncate())
}
fn button(key: &str, value: &str, p: Paint, f: impl Fn(&mut State) + 'static) -> El {
    row().key(key).role("button").label(value).focusable(true).h(29.0).px(10.0).center().radius(7.0).fill(p.card).border(0.5,p.line).hover(p.hover).on_press(f).child(label(value,11.5,500.0,p.secondary))
}
fn avatar(login: &str, size: f32, p: Paint) -> El {
    let initial = login.chars().next().unwrap_or('?').to_uppercase().to_string();
    let colors = ["#8b85b5","#729a8f","#bc9b78","#849aba","#b7879d"];
    let color = c(colors[login.bytes().fold(0usize,|s,b|s.wrapping_add(b as usize))%colors.len()]);
    row().size(size,size).radius(size/2.0).fill(color.alpha(if p.dark {0.35} else {0.18})).center().child(label(initial,size*0.43,600.0,color))
}
fn profile(s:&State,login:&str,size:f32,font:f32,key:&str,p:Paint)->El {
    let normalized=tracker_core::customer_config::normalize_login(login).ok();
    let Some(login_value)=normalized else{return row().gap(6.0).center_y().child(avatar(login,size,p)).child(label(login,font,450.0,p.secondary).truncate());};
    let saved=s.cust_prefs.rows.iter().find(|p|p.login.eq_ignore_ascii_case(login));
    let author=s.detail.iter().chain(s.items.iter()).chain(s.dev.items.iter()).filter_map(|i|i.author.as_ref()).find(|a|a.customer&&a.login.eq_ignore_ascii_case(login));
    let (name,company,notes)=if let Some(p)=saved{(p.full_name.as_str(),p.company.as_str(),p.notes.as_str())}else if let Some(a)=author{(a.full_name.as_str(),a.company.as_str(),a.notes.as_str())}else{("","","")};
    let name=if name.trim().is_empty(){login}else{name};
    let display=if company.trim().is_empty(){name.to_owned()}else{format!("{name} ({company})")};
    let mut help=format!("{display}\n@{login}");
    if !notes.trim().is_empty(){help.push_str(&format!("\n\n{notes}"));}
    help.push_str("\n\nClick to open GitHub · Right-click for VIP actions");
    let open_login=login_value.clone();let menu_login=login_value;
    let vip=s.data.vip_logins.iter().any(|l|l.eq_ignore_ascii_case(login))
        || s.cust_prefs.rows.iter().any(|p|p.login.eq_ignore_ascii_case(login))
        || s.items.iter().chain(s.dev.items.iter()).filter_map(|i|i.author.as_ref()).any(|a|a.customer&&a.login.eq_ignore_ascii_case(login));
    row().key(key).role("link").label(format!("Open {login} on GitHub")).help(help).focusable(true).cursor(rune::Cursor::Pointer).hover_group_focus().gap(6.0).center_y().min_w(0.0)
        .on_press_tx(move |_:&mut State,tx|{let _=tx.open_url(&format!("https://github.com/{open_login}"));})
        .on_secondary_press(move |_:&mut State,press,tx|{
            let login=menu_login.clone();
            tx.popup_menu(rune::PopupMenu::new().item(rune::PopupItem::new("profile","Open GitHub profile")).separator().item(rune::PopupItem::new("vip",if vip{"Edit VIP…"}else{"Add to VIPs…"})),press,
                move |s:&mut State,picked,tx|match picked{Some("profile")=>{let _=tx.open_url(&format!("https://github.com/{login}"));},Some("vip")=>s.open_vip_profile(login,tx),_=>{}});
        })
        .child(avatar(login,size,p)).child(label(display,font,450.0,p.secondary).group_color(p.accent).truncate())
}
fn nav(key: &str, name: &str, count: impl ToString, glyph: &'static Icon, color: Color, active: bool, p: Paint, f: impl Fn(&mut State) + 'static) -> El {
    row().key(key).role("button").label(name).focusable(true).h(33.0).w_full().px(9.0).gap(9.0).center_y().radius(7.0)
        .fill(if active {p.selected} else {Color::TRANSPARENT}).hover(p.hover).on_press(f)
        .child(icon(glyph,16.0).color(if active {p.accent} else {color}))
        .child(label(name,12.5,if active {550.0} else {400.0},if active {p.ink} else {p.secondary}).grow(1.0).truncate())
        .child(label(count.to_string(),11.0,500.0,if active {p.accent} else {p.muted}))
}
fn sidebar(s: &State, p: Paint) -> El {
    let total = s.data.items().iter().filter(|i|s.followup_matches(i)).count();
    let mut navigation = col().key("pen-navigation").gap(3.0)
        .child(nav("nav-all","All external",total,&INBOX,p.accent,!s.dev.active&&s.pen.is_empty(),p,|s| s.followup("")));
    let colors = ["#5a9d83","#7a94bf","#a391c2","#bb9b70","#96979e"];
    let names = ["Last 24 hours","Last week","Last month","Last 6 months","Cold & stale"];
    for (index, pen) in s.data.pens.iter().enumerate() {
        let id = pen.id.clone();
        let count = pen.items.iter().filter(|i|s.followup_matches(i)).count();
        navigation = navigation.child(nav(&format!("nav-{}",pen.id),names.get(index).copied().unwrap_or(&pen.label),count,&CLOCK,c(colors[index%5]),!s.dev.active&&s.pen==pen.id,p,move |s| s.followup(&id)));
    }
    let mut developer_nav=col().key("developer-navigation").gap(3.0).pt(20.0)
        .child(label("DEVELOPER WORK",9.5,600.0,p.muted).px(9.0).pb(6.0)).child(label(&s.dev.login,11.0,500.0,p.secondary).px(9.0).pb(5.0));
    for (queue,title) in developer::QUEUES {
        let known=s.dev.items.iter().filter(|i|developer::matches(i,queue,if matches!(queue,"review"|"assigned"){false}else{s.dev.closed})).count();
        let count=if s.dev.loaded_queues.iter().any(|q|q==queue){known.to_string()}else if known>0{format!("{known}+")}else{"…".into()};
        developer_nav=developer_nav.child(nav(&format!("dev-{queue}"),title,count,&PR,p.purple,s.dev.active&&s.dev.queue==queue,p,move|s|s.developer_queue(queue)));
    }
    let (dot,status) = if s.auth.disconnected {(c("#bd9b61"),"GitHub disconnected · saved data available")} else {match &s.connection { Connection::Online => (if s.data.backend.polling {p.green}else{c("#bd9b61")},s.data.backend.message.as_str()), Connection::Cached => (c("#bd9b61"),"Saved snapshot"), Connection::Offline(_) => (c("#d18d6c"),"Local tracker unavailable") }};
    // Native materials inherit macOS appearance rather than the app's appearance toggle.
    // An opaque surface keeps the sidebar and its text in the same theme in both modes.
    col().key("sidebar").w(220.0).h_full().min_h(0.0).fill(p.side).border_sides([0.0,0.5,0.0,0.0],p.line)
        .child(row().key("sidebar-titlebar").h(60.0).w_full().window_drag())
        .child(row().px(18.0).pb(25.0).gap(10.0).center_y()
            .child(icon(&THREADKEEPER,36.0).label("Threadkeeper logo"))
            .child(col().gap(3.0).child(label("Threadkeeper",16.0,650.0,p.ink)).child(label("Keep threads moving.",10.5,400.0,p.secondary))))
        .child(col().key("sidebar-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).px(12.0)
            .child(label("FOLLOW-UP",9.5,600.0,p.muted).pl(9.0).pb(9.0)).child(navigation).child(developer_nav)
            .children(s.auth.disconnected.then(||label("GitHub disconnected",11.0,500.0,p.secondary)))
            .children(s.auth.disconnected.then(||button("reconnect-github","Reconnect GitHub",p,|s|{s.auth.started=true;s.auth.reconnect_open=true;}).w_full()))
            .child(label(if s.dev.active{"+ = partial count · … = not loaded"}else{"Counts match the current scope"},10.0,400.0,p.muted).px(9.0).pt(13.0).w_full().wrap()))
        .child(col().padding(17.0).gap(9.0).child(button("manage-repositories","Settings…",p,|s|s.open_repository_preferences())).child(rule(p))
            .child(row().pt(6.0).gap(7.0).center_y().child(el().size(5.0,5.0).radius(3.0).fill(dot)).child(label(status,10.0,400.0,p.secondary).grow(1.0).min_w(0.0).wrap()))
            .child(label(format!("{} maintainers excluded",s.data.maintainers),10.0,400.0,p.muted).pl(12.0))
            .children(match &s.connection {Connection::Offline(message)=>Some(label(message,9.5,400.0,c("#d18d6c")).w_full().wrap()),_=>None})
            .children(s.data.backend.config_error.as_ref().map(|error|label(error,9.5,400.0,c("#d18d6c")).w_full().wrap())).label(match &s.connection {Connection::Offline(message)=>message.as_str(),_=>"Embedded Rust tracker"}))
}
fn entry(s: &State, index: usize, p: Paint) -> El {
    let item = &s.items[index]; let id = item.id.clone(); let selected = item.id==s.selected;
    let card_url=item.url.clone();let menu_url=item.url.clone();
    let is_pr = item.kind=="pr"; let color = if is_pr {p.purple} else {p.green};
    let author = item.author.as_ref().map(|a|a.login.as_str()).unwrap_or("Unknown author");
    let customer=item.author.as_ref().filter(|a|a.customer);
    let style=customer.map(|a|a.highlight.as_str()).unwrap_or("");
    let highlight=if style=="neon"{c("#00dca0")}else{c("#c99728")};
    row().key(format!("entry-{}",item.id)).role("button").label(&item.title).focusable(true).hover_group_focus().w_full().min_h(112.0).px(15.0).py(12.0).gap(10.0)
        .fill(if selected {p.selected} else if style=="subtle-tint" {p.accent.alpha(0.08)} else {Color::TRANSPARENT}).hover(p.hover).border_sides(if style=="outline"{[1.5;4]}else if style=="neon"{[0.0,0.0,0.5,4.0]}else{[0.0,0.0,0.5,0.0]},if style=="outline"||style=="neon"{highlight}else{p.line.alpha(0.8)})
        .on_press(move |s: &mut State| s.select(id.clone()))
        .on_secondary_press(move |_: &mut State,press,tx| {
            let url=menu_url.clone();
            let menu=rune::PopupMenu::new().item(rune::PopupItem::new("open-github","Open in GitHub").enabled(url.starts_with("https://github.com/")));
            tx.popup_menu(menu,press,move |_: &mut State,picked,tx| {if picked==Some("open-github") && url.starts_with("https://github.com/"){let _=tx.open_url(&url);}});
        })
        .child(icon(if is_pr {&PR} else {&ISSUE},16.0).color(color).mt(2.0))
        .child(col().grow(1.0).min_w(0.0).gap(7.0)
            .child(row().gap(6.0).center_y().child(label(data::repo_name(&item.repo),10.5,500.0,p.secondary).grow(1.0).truncate())
                .children(customer.map(|_|pill(if style=="gold-star"{"★ VIP"}else{"VIP"},highlight)))
                .children(is_pr.then(||pill(if item.state=="draft" {"Draft PR"} else {"PR"},p.purple).key(format!("pr-badge-{}",item.id))))
                .children((s.dev.active&&matches!(item.state.as_str(),"closed"|"merged")).then(||pill(&item.state,p.secondary)))
                .child(label(s.age(if s.dev.active{developer::sort_at(item,&s.dev.queue)}else{&item.engagement.at}),10.0,400.0,p.muted)))
            .child(label(&item.title,12.8,if selected {550.0} else {450.0},p.ink).w_full().wrap())
            .children((s.dev.active&&!item.developer_reasons.is_empty()).then(||row().key(format!("developer-badges-{}",item.id)).w_full().gap(5.0).flex_wrap().children(item.developer_reasons.iter().map(|r|pill(developer::reason(r),p.accent)))))
            .children((!item.labels.is_empty()).then(||row().key(format!("entry-labels-{}",item.id)).w_full().gap(5.0).flex_wrap().children(item.labels.iter().map(|l|label_badge(l,p)))))
            .child(row().gap(5.0).center_y().child(profile(s,author,16.0,10.5,&format!("card-profile-{}",item.id),p)).child(el().grow(1.0))
                .children((!item.advice.text.is_empty()).then(||icon(&NOTE,12.0).color(p.accent)))
                .children((item.comments>0).then(||row().key(format!("entry-comments-{}",item.id)).gap(4.0).center_y()
                    .label(format!("{} discussion comment{}",item.comments,if item.comments==1 {""}else{"s"}))
                    .child(icon(&COMMENT,12.0).color(p.muted)).child(label(item.comments.to_string(),10.0,400.0,p.muted)).mr(5.0)))
                .child(label(format!("#{}",item.number),10.0,400.0,p.muted))
                .child(row().key(format!("entry-open-{}",item.id)).cursor(rune::Cursor::Pointer).role("button").label(format!("Open {} #{} in GitHub",item.repo,item.number)).focusable(true).size(26.0,26.0).radius(5.0).center().hover(p.hover).group_opacity(if selected{1.0}else{0.0},1.0).disabled(!card_url.starts_with("https://github.com/")).on_press_tx(move |_: &mut State,tx|{if card_url.starts_with("https://github.com/"){let _=tx.open_url(&card_url);}}).child(icon(&EXTERNAL,13.0).color(p.accent)))))
}
fn list(s: &State, p: Paint) -> El {
    let title = if s.dev.active {developer::title(&s.dev.queue).into()} else if s.pen.is_empty() {"All external".into()} else {s.data.pens.iter().find(|p|p.id==s.pen).map(|p|p.label.clone()).unwrap_or_default()};
    let refresh = row().key("refresh").role("button").label("Refresh from GitHub · Command R").focusable(true).size(30.0,30.0).radius(7.0).hover(p.hover).center().disabled(if s.dev.active{s.dev.loading||s.dev.login.is_empty()}else{s.refreshing||s.repo_scope.selected(&s.repositories()).is_empty()}).on_press(|s: &mut State|s.hard_refresh()).child(icon(&REFRESH,16.0).color(if s.refreshing {p.muted}else{p.secondary}));
    let search = row().h(32.0).px(9.0).gap(7.0).radius(7.0).fill(p.card).border(0.5,p.line).center_y()
        .child(icon(&SEARCH,14.0).color(p.muted))
        .child(text_input(&s.search,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS"))).key("search").grow(1.0).single_line().color(p.ink).placeholder("Search issues and pull requests",p.muted).label("Search issues and pull requests").on_edit(|s: &mut State| &mut s.search).on_input_change(|s: &mut State,_|s.refilter()))
        .child(label("⌘F",10.0,400.0,p.muted));
    let mut types = row().gap(3.0).center_y();
    for (key,name) in [("","All"),("issue","Issues"),("pr","Pull requests")] {
        let active = s.kind==key;
        types = types.child(row().key(format!("type-{name}")).role("button").label(name).focusable(true).h(26.0).px(10.0).center_y().radius(6.0).fill(if active {p.selected} else {Color::TRANSPARENT}).hover(p.hover)
            .on_press(move |s: &mut State| {s.kind=key.into();s.refilter();}).child(label(name,11.0,if active {550.0} else {400.0},if active {p.accent} else {p.secondary})));
    }
    let narrowed = !s.kind.is_empty() || !s.search.text.is_empty();
    let available=s.repositories();
    let repository=s.repo_scope.summary(&available);
    let scope = col().key("repository-scope").gap(7.0)
        .child(label("REPOSITORIES",9.5,600.0,p.muted))
        .child(row().key("repository-picker").role("button").label(format!("Repositories: {repository}")).focusable(true).expanded(s.repo_picker_open)
            .w_full().h(34.0).px(10.0).gap(8.0).center_y().radius(7.0).fill(p.card).border(0.5,p.line).hover(p.hover)
            .on_press_tx(|s: &mut State,tx| {s.repo_picker_open=!s.repo_picker_open;s.repo_search=rune::TextEdit::new("");if s.repo_picker_open {tx.focus("repo-scope-search",true);}})
            .child(icon(&FOLDER,14.0).color(p.secondary))
            .child(label(&repository,12.0,500.0,p.ink).grow(1.0).truncate())
            .child(label("⌃⌄",10.0,500.0,p.muted)));
    let scope=if s.dev.active {col().gap(9.0).child(developer_profile(s,p,"developer-profile"))
        .child(label("All enabled repositories · title filters ignored",10.5,400.0,p.muted).w_full().wrap())
        .child(button("developer-history",if s.dev.closed{"✓ Include closed/merged"}else{"Include closed/merged"},p,|s|{s.remember_note();s.dev.closed=!s.dev.closed;s.dev.loaded_queues.clear();s.dev.checked=None;s.dev.loading=false;s.dev.items.clear();s.items.clear();s.refilter();s.load_developer(false);}))}else{scope};
    let reset=narrowed.then(||row().key("active-filters").gap(8.0).center_y()
        .child(label("Search & type filters applied",10.0,400.0,p.muted).grow(1.0))
        .child(button("reset-list-filters","Reset search & type",p,|s| {s.kind.clear();s.search=rune::TextEdit::new("");s.refilter();})));
    let content = if s.visible.is_empty() {
        let none_selected=!s.dev.active&&s.repo_scope.selected(&available).is_empty();
        let message=if s.dev.active {if s.dev.login.is_empty(){"Add a maintainer in Settings to view their work.".into()}else if s.dev.loading{"Searching GitHub across enabled repositories…".into()}else if !s.dev.loaded_queues.contains(&s.dev.queue){"This queue has not finished loading. Saved data will appear when available.".into()}else{"No matching developer work. Refresh to check GitHub again.".into()}}else if none_selected {"Select one or more repositories to see their issues and PRs.".into()}else if narrowed {format!("No matches in {repository} with these filters.")}else{format!("No open external entries in {repository} for this pen.")};
        col().key("empty-followup").grow(1.0).justify_center().items_center().px(26.0).gap(12.0)
            .child(icon(&INBOX,30.0).color(p.muted))
            .child(label(if s.dev.active{"Developer work"}else{"Nothing to follow up here"},14.0,550.0,p.secondary))
            .child(label(message,11.0,400.0,p.muted).w_full().wrap())
            .children(none_selected.then(||button("choose-repositories","Select repositories",p,|s|{s.repo_picker_open=true;})))
            .children((!s.dev.active&&!none_selected&&!s.pen.is_empty()).then(||button("view-all-followup","View all follow-up",p,|s| {s.pen.clear();s.refilter();})))
    } else { col().key("entry-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(col().key("entries").growth(GrowthSpec::default()).virtual_rows(s.visible.len(),112.0,
        |s: &State,i| format!("entry-{}",s.items[s.visible[i]].id).into(), move |s: &State,i|entry(s,s.visible[i],p))) };
    let refresh_status=s.refresh_status.as_ref().map(|status| {
        let color=if status.status=="failed" {c("#d18d6c")}else if s.refreshing {p.accent}else{p.green};
        let scope=status.repositories.as_ref().map(|repos|if repos.len()==1 {data::repo_name(&repos[0]).to_owned()}else{format!("{} repositories",repos.len())}).unwrap_or_else(||"All repositories".into());
        col().key("github-refresh-status").px(18.0).pb(12.0).gap(4.0)
            .child(label(if s.refreshing {format!("Refreshing GitHub · {scope}")} else {status.message.clone()},10.5,500.0,color).w_full().wrap())
            .children(s.refreshing.then(||label(&status.message,10.0,400.0,p.secondary).w_full().wrap()))
    });
    col().key("list-pane").w(425.0).h_full().min_h(0.0).fill(p.bg).border_sides([0.0,0.5,0.0,0.0],p.line)
        .child(row().h(60.0).px(20.0).gap(8.0).center_y().window_drag().child(label(if s.dev.active{"Developer work"}else{"Follow-up"},16.0,650.0,p.ink).grow(1.0)).child(refresh))
        .child(col().px(18.0).pb(14.0).gap(12.0).child(scope).child(search).child(row().center_y().justify_between().child(types).child(button("order",if s.newest {"Newest ↓"}else{"Oldest ↑"},p,|s|{s.newest=!s.newest;s.refilter();}))))
        .children(reset.map(|reset|reset.px(18.0).pb(12.0)))
        .children(refresh_status)
        .child(if s.dev.active {developer_status(s,p)}else{automatic_sync_status(s,p)})
        .child(rule(p)).child(row().h(39.0).px(19.0).center_y().child(label(title,11.0,550.0,p.secondary).grow(1.0).truncate()).child(label(if s.dev.active&&!s.dev.loaded_queues.contains(&s.dev.queue){format!("{} cached · incomplete",s.visible.len())}else{format!("{} {}",s.visible.len(),if s.visible.len()==1 {"entry"} else {"entries"})},10.5,400.0,p.muted)))
        .child(content)
        .child(row().h(31.0).px(17.0).center_y().border_sides([0.5,0.0,0.0,0.0],p.line).child(label("↑ ↓  Navigate",9.5,400.0,p.muted).grow(1.0)).child(label("⌘O  Open on GitHub",9.5,400.0,p.muted)))
}
fn metadata(name: &str, value: El, p: Paint) -> El { row().w_full().min_h(27.0).gap(10.0).center_y().child(label(name,11.5,400.0,p.secondary).w(112.0)).child(value) }
fn description(body: &str, p: Paint, key: &str) -> El {
    let mut result = col().key(key).w_full().gap(12.0).selection_group(true);
    if body.is_empty() { return result.child(label("No description provided.",12.5,400.0,p.muted)); }
    let mut code = false;
    for (index, paragraph) in body.split("\n\n").enumerate() {
        let part = paragraph.trim(); if part.is_empty(){continue;}
        let has_fence=part.contains("```");
        if has_fence || code {
            let value = part.replace("```typescript", "").replace("```javascript", "").replace("```dart", "").replace("```", "");
            result=result.child(col().key(format!("{key}-code-{index}")).w_full().padding(12.0).radius(7.0).fill(p.card).child(text(value).font(TextStyle::new(11.0,400.0).family(Family::SystemMono).lh(17.0)).color(p.secondary).w_full().wrap().selectable(true)));
            if part.matches("```").count()%2==1{code=!code;}
        } else {
            let heading=part.starts_with('#');
            let value=part.trim_start_matches('#').trim().replace("**", "").replace("<!--", "").replace("-->", "");
            result=result.child(label(value,if heading{15.0}else{12.5},if heading{600.0}else{400.0},if heading{p.ink}else{p.secondary}).key(format!("{key}-paragraph-{index}")).w_full().wrap().selectable(true));
        }
    }
    result
}
fn discussion(s: &State, item: &data::Item, p: Paint, cx:&mut Cx<State>) -> El {
    let state=s.discussions.get(&item.id);
    let mut view=col().key("discussion").w_full().gap(14.0).child(rule(p))
        .child(row().gap(7.0).center_y().child(icon(&COMMENT,14.0).color(p.muted))
            .child(label(format!("DISCUSSION · {}",item.comments),9.5,600.0,p.muted).grow(1.0))
            .children((item.comments>0||item.kind=="pr").then(||button("refresh-comments","Refresh",p,|s|s.load_discussion(1,true)).disabled(state.is_some_and(|s|s.loading)))));
    if let Some(state)=state {
        for comment in &state.comments {
            let url=comment.url.clone();let highlighted=s.dev.active&&comment.author.eq_ignore_ascii_case(&s.dev.login);
            let display_key=format!("{}:{}:{}",item.id,comment.kind,comment.id);
            let expanded=s.comment_display.get(&display_key).is_some_and(|d|d.expanded);
            let collapsed=s.comment_display.get(&display_key).is_some_and(|d|d.collapsed);
            let toggle_key=display_key.clone();let more_key=display_key.clone();
            let card_key=format!("comment-{}{}",if comment.kind=="review"{"review-"}else{""},comment.id);
            let width=cx.layout_size(&card_key).map(|r|(r[0]-26.0).max(1.0));
            let style=TextStyle::new(12.5,400.0).lh(17.75);
            let body=tracker_core::discussion::display_body(&comment.body);
            let body=if body.is_empty(){if comment.kind=="review"{"No review summary. Inline code comments, if any, are available on GitHub.".to_owned()}else{"This comment has no text.".to_owned()}}else{body};
            let long=width.is_some_and(|w|rune::text::measure(&body,&[],style,Some(w)).1>5.0*17.75+0.5);
            let header_key=format!("comment-header-{display_key}");let collapse_anchor=header_key.clone();let more_anchor=header_key.clone();
            view=view.child(col().key(format!("comment-{}{}",if comment.kind=="review"{"review-"}else{""},comment.id)).instant_changes().w_full().padding(13.0).gap(10.0).radius(8.0).fill(if highlighted{p.accent.alpha(0.12)}else{p.card})
                .children(highlighted.then(||pill(&format!("{} · selected developer",s.dev.login),p.accent).key(format!("selected-developer-{}-{}",comment.kind,comment.id))))
                .children((comment.kind=="review").then(||pill(&format!("Review · {}",comment.review_state.replace('_'," ")),p.purple)))
                .child(row().key(header_key).gap(7.0).center_y().child(profile(s,&comment.author,20.0,11.5,&format!("comment-profile-{}-{}",comment.kind,comment.id),p))
                    .child(label(format!("{}{}",data::date(&comment.created_at),if comment.updated_at!=comment.created_at{" · edited"}else{""}),10.0,400.0,p.muted).grow(1.0))
                    .child(button(&format!("collapse-comment-{}-{}",comment.kind,comment.id),if collapsed{"Expand comment"}else{"Collapse"},p,|_|{}).on_press_tx(move |s:&mut State,tx|{let d=s.comment_display.entry(toggle_key.clone()).or_default();d.collapsed=!d.collapsed;tx.keep_on_screen(collapse_anchor.clone());tx.instant();}))
                    .child(row().key(format!("open-comment-{}{}",if comment.kind=="review"{"review-"}else{""},comment.id)).role("button").label("Open comment in GitHub").focusable(true).size(24.0,24.0).center().radius(4.0).hover(p.hover).on_press_tx(move |_: &mut State,tx|{if url.starts_with("https://github.com/"){let _=tx.open_url(&url);}}).child(icon(&EXTERNAL,12.0).color(p.secondary))))
                .children((!collapsed).then(|| {
                    let key=format!("comment-body-{}-{}",comment.kind,comment.id);
                    col().w_full().gap(7.0)
                        .child(text(&body).key(key).font(style).color(p.secondary).w_full().max_lines(if expanded{0}else{5}).selectable(true))
                        .children((long||expanded).then(||button(&format!("more-comment-{}-{}",comment.kind,comment.id),if expanded{"See less"}else{"See more"},p,|_|{}).on_press_tx(move |s:&mut State,tx|{let d=s.comment_display.entry(more_key.clone()).or_default();d.expanded=!d.expanded;tx.keep_on_screen(more_anchor.clone());tx.instant();})))
                })));

        }
        if let Some(warning)=&state.warning {view=view.child(label(warning,11.0,400.0,p.secondary).w_full().wrap());}
        if let Some(error)=&state.error {view=view.child(label(error,11.0,400.0,p.secondary).w_full().wrap()).child(button("retry-comments","Retry",p,|s|s.load_discussion(1,true)));}
        if state.loading {view=view.child(label("Loading comments…",11.5,400.0,p.muted));}
        else if let Some(page)=state.next {view=view.child(button("more-comments","Load more comments",p,move |s|s.load_discussion(page,false)));}
        else if state.comments.is_empty()&&state.error.is_none() {view=view.child(label("No discussion comments yet",11.5,400.0,p.muted));}
    } else {view=view.child(label(if item.comments==0{"No discussion comments yet"}else{"Loading comments…"},11.5,400.0,p.muted));}
    if item.kind=="pr" {view=view.child(label("Review summaries included. Inline code threads are available on GitHub.",11.0,400.0,p.muted).w_full().wrap());}
    view
}
fn preview_navigation(s:&State,p:Paint)->Option<El> {
    s.preview_id.as_ref().map(|_|col().key("connection-preview").transition(rune::Transition::none()).instant_changes().motion_intent(rune::MotionIntent::CUT).w_full().px(29.0).py(8.0).gap(5.0).fill(p.card)
        .child(button("back-to-original","← Back to original entry",p,|s|s.back_to_original()))
        .children(s.detail_error.as_ref().map(|error|label(error,11.0,400.0,p.secondary).w_full().wrap())))
}
fn detail(s: &State, p: Paint, cx:&mut Cx<State>) -> El {
    let current = s.current_item();
    let mut toolbar = row().h(60.0).px(27.0).gap(12.0).center_y().window_drag()
        .child(label("ITEM DETAILS",9.5,550.0,p.muted).grow(1.0))
        .child(row().key("toggle-advice").role("button").label("Show or hide notes").focusable(true).size(28.0,28.0).radius(6.0).hover(p.hover).center().on_press(|s: &mut State|s.show_advice=!s.show_advice).child(icon(&NOTE,16.0).color(if s.show_advice{p.accent}else{p.muted})))
        .child(row().key("appearance").role("button").label("Toggle appearance").focusable(true).size(28.0,28.0).radius(6.0).hover(p.hover).center().on_press(move |s: &mut State|s.appearance=Some(!p.dark)).child(icon(&SUN,16.0).color(p.secondary)));
    if let Some(item) = current { let url = item.url.clone(); toolbar=toolbar.child(row().key("open-github").role("button").label("Open on GitHub").focusable(true).h(29.0).px(10.0).gap(6.0).center_y().radius(7.0).border(0.5,p.line).fill(p.card).hover(p.hover).on_press_tx(move |_: &mut State,tx| {if url.starts_with("https://github.com/"){let _=tx.open_url(&url);}}).child(label("Open in GitHub",11.0,500.0,p.secondary)).child(icon(&EXTERNAL,12.0).color(p.secondary))); }
    let Some(item) = current else { return col().key("detail-pane").grow(1.0).h_full().fill(p.bg).child(toolbar).children(preview_navigation(s,p)).child(col().grow(1.0).center().gap(12.0).child(icon(&INBOX,36.0).color(p.muted)).child(label(if s.preview_id.is_some(){"Connected entry"}else{"Select an entry"},18.0,550.0,p.secondary)).child(label(s.detail_error.as_deref().unwrap_or(if s.preview_id.is_some(){"Loading connected entry…"}else{"Its context and connections will appear here."}),12.0,400.0,p.muted).w_full().wrap())); };
    let is_pr=item.kind=="pr"; let color=if is_pr{p.purple}else{p.green}; let author=item.author.as_ref().map(|a|a.login.as_str()).unwrap_or("Unknown author");
    let pen=if s.preview_id.is_some()&&!s.items.iter().any(|i|i.id==item.id){"Outside follow-up view".into()}else{data::pen_for(&s.data,&item.id)};
    let mut content = col().key(format!("detail-content-{}",item.id)).w_full().px(29.0).pt(9.0).pb(30.0).gap(18.0)
        .child(row().gap(6.0).center_y().child(icon(&FOLDER,13.0).color(p.muted)).child(label(data::repo_name(&item.repo),11.0,400.0,p.secondary)).child(icon(&CHEVRON,9.0).color(p.muted)).child(label(format!("#{}",item.number),11.0,400.0,p.muted)).child(el().grow(1.0)).child(pill(if is_pr{"Pull request"}else{"Issue"},color)))
        .child({let url=item.url.clone();row().key("detail-title-link").cursor(rune::Cursor::Pointer).role("link").label(format!("Open {} in GitHub",item.title)).focusable(true).w_full().gap(9.0).hover_group_focus().disabled(!url.starts_with("https://github.com/")).on_press_tx(move |_: &mut State,tx|{if url.starts_with("https://github.com/"){let _=tx.open_url(&url);}})
            .child(label(&item.title,24.0,650.0,p.ink).key("detail-title").grow(1.0).min_w(0.0).wrap().group_color(p.accent))
            .child(icon(&EXTERNAL,16.0).color(p.muted).group_color(p.accent).mt(8.0))})
        .child(row().gap(8.0).center_y().child(profile(s,author,25.0,12.0,"detail-author-profile",p)).children(item.author.as_ref().filter(|a|a.customer).map(|_|label("★ VIP",11.0,500.0,c("#c99728")))).child(label(format!("opened on {}",data::date(&item.created_at)),11.0,400.0,p.secondary)))
        .children(item.author.as_ref().filter(|a|a.customer&&!a.company.trim().is_empty()).map(|a|metadata("Company",label(&a.company,11.5,400.0,p.secondary).grow(1.0).wrap(),p)))
        .children(item.author.as_ref().filter(|a|a.customer&&!a.notes.trim().is_empty()).map(|a|col().key("vip-detail-notes").w_full().gap(4.0).padding(12.0).radius(7.0).fill(p.card)
            .child(label("VIP notes",10.5,550.0,p.accent)).child(label(&a.notes,11.5,400.0,p.secondary).w_full().wrap().selectable(true))))
        .child(rule(p))
        .child(col().gap(7.0)
            .child(metadata("Status",pill(match item.state.as_str(){"draft"=>"Draft","closed"=>"Closed","merged"=>"Merged",_=>"Open"},color),p))
            .child(metadata(if s.dev.active{"Developer"}else{"Engagement pen"},row().gap(6.0).center_y().child(icon(&CLOCK,12.0).color(p.accent)).child(label(pen,11.5,500.0,p.ink)),p))
            .child(metadata("Last engagement",label(if item.engagement.pending{"Awaiting initial sync".into()}else{format!("{} ago · {}",s.age(&item.engagement.at),item.engagement.kind)},11.5,400.0,p.ink),p))
            .child(metadata("Assignee",label(if item.assignees.is_empty(){"Unassigned".into()}else{item.assignees.iter().map(|p|p.login.as_str()).collect::<Vec<_>>().join(", ")},11.5,400.0,p.secondary),p))
            .child(metadata("Labels",row().grow(1.0).min_w(0.0).gap(5.0).flex_wrap().children(item.labels.iter().map(|l|label_badge(l,p))),p)))
        .child(rule(p));
    if s.show_advice {
        let status=match &s.note_state { NoteState::Clean=>"Private to this tracker",NoteState::Dirty=>"Unsaved changes",NoteState::Saving=>"Saving…",NoteState::Saved=>"Saved locally",NoteState::Failed(message)=>message.as_str() };
        let input = text_input(&s.note,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS")).lh(19.0)).key("advice-input").w_full().min_h(160.0).wrap().color(p.ink).placeholder("Add a note about this entry…",p.muted).label("Notes").on_edit(|s: &mut State| &mut s.note).on_input_change(|s: &mut State,_|s.note_state=NoteState::Dirty);
        content=content.child(col().key("advice-card").padding(14.0).gap(10.0).w_full().radius(10.0).continuous_corners().fill(p.accent.alpha(if p.dark{0.075}else{0.035})).border(0.5,p.accent.alpha(0.18))
            .child(row().gap(7.0).center_y().child(icon(&NOTE,15.0).color(p.accent)).child(label("Notes",12.0,550.0,p.accent)).child(el().grow(1.0)).child(label("⌘S",10.0,400.0,p.muted)))
            .child(col().key("notes-scroll").w_full().h(160.0).min_h(160.0).max_h(160.0).scroll(ScrollSpec { stick_bottom: false }).child(input))
            .child(row().gap(7.0).center_y().child(el().grow(1.0)).child(label(status,10.0,400.0,p.muted)).child(button("save-advice","Save",p,|s|s.save()))));
    }
    if !item.related.is_empty() {
        let mut ids=std::collections::HashSet::new(); let mut related=col().gap(8.0).child(label("CONNECTIONS",9.5,600.0,p.muted));
        for link in item.related.iter().filter(|link|ids.insert(format!("{}#{}",link.repo,link.number))) {
            let id=link.id.clone(); let url=link.url.clone();
            let external_url=url.clone();
            related=related.child(row().key(format!("connection-{}-{}",link.repo,link.number)).gap(8.0).px(10.0).py(9.0).radius(6.0).fill(p.card).center_y()
                .child(row().key(format!("related-{}-{}",link.repo,link.number)).role("button").label("View connected entry").focusable(true).gap(8.0).grow(1.0).min_w(0.0).center_y().hover(p.hover).on_press_tx(move |s: &mut State,tx|{if let Some(id)=&id{s.preview_connection(id.clone());}else if url.starts_with("https://github.com/"){let _=tx.open_url(&url);}})
                    .child(icon(if link.kind=="issue"{&ISSUE}else{&PR},13.0).color(p.purple)).child(label(link.title.clone().unwrap_or_else(||format!("{} #{}",data::repo_name(&link.repo),link.number)),11.0,400.0,p.secondary).grow(1.0).truncate()).child(icon(&CHEVRON,10.0).color(p.muted)))
                .child(row().key(format!("open-related-{}-{}",link.repo,link.number)).role("button").label("Open connection in GitHub").focusable(true).h(26.0).px(7.0).gap(5.0).center_y().radius(5.0).hover(p.hover).on_press_tx(move |_: &mut State,tx|{if external_url.starts_with("https://github.com/"){let _=tx.open_url(&external_url);}}).child(label("Open in GitHub",10.0,500.0,p.secondary)).child(icon(&EXTERNAL,12.0).color(p.secondary))));
        }
        content=content.child(related);
    }
    content=content.child(label("DESCRIPTION",9.5,600.0,p.muted)).children((s.dev.active&&item.author.as_ref().is_some_and(|a|a.login.eq_ignore_ascii_case(&s.dev.login))).then(||pill(&format!("{} · author",s.dev.login),p.accent))).child(description(&item.body,p,"description")).child(discussion(s,item,p,cx));
    col().key("detail-pane").grow(1.0).min_w(0.0).h_full().min_h(0.0).fill(p.bg).child(toolbar).children(preview_navigation(s,p)).child(col().key("detail-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).scroll_anchor().child(content))
}
fn repository_popover(s:&State,p:Paint)->El {
    let available=s.repositories();let search=s.repo_search.text.to_lowercase();
    let matching=available.iter().filter(|repo|repo.to_lowercase().contains(&search)).collect::<Vec<_>>();
    let mut options=col().key("repo-scope-options").gap(2.0);
    for repo in &matching {
        let selected=s.repo_scope.includes(repo);let toggle=(*repo).clone();let only=(*repo).clone();
        options=options.child(row().key(format!("repo-option-{repo}")).gap(5.0).center_y()
            .child(row().key(format!("repo-check-{repo}")).role("checkbox").label((*repo).clone()).checked(selected).focusable(true)
                .grow(1.0).min_w(0.0).h(34.0).px(7.0).gap(9.0).center_y().radius(6.0).hover(p.hover)
                .on_press(move|s:&mut State|s.toggle_repo(&toggle))
                .child(row().size(15.0,15.0).center().radius(4.0).border(0.8,if selected{p.accent}else{p.muted}).fill(if selected{p.accent}else{Color::TRANSPARENT})
                    .children(selected.then(||label("✓",11.0,650.0,if p.dark{p.bg}else{Color::WHITE}))))
                .child(label(data::repo_name(repo),11.5,450.0,p.ink).grow(1.0).truncate()))
            .child(button(&format!("repo-only-{repo}"),"Only",p,move|s|s.set_repo_scope(crate::RepoScope::only(only.clone()))).label(format!("Only this repo: {repo}"))));
    }
    if matching.is_empty(){options=options.child(label("No repositories match.",11.5,400.0,p.muted).padding(12.0));}
    let selected=s.repo_scope.selected(&available).len();
    col().key("repository-popover").role("dialog").label("Choose repositories").focus_return("repository-picker")
        .modal(true,|s:&mut State|s.repo_picker_open=false)
        .transition(rune::Transition::none()).instant_changes().motion_intent(rune::MotionIntent::CUT)
        .anchored_fit("repository-picker",rune::Side::Bottom,rune::Align::Start,6.0).w(389.0).max_h(470.0).padding(12.0).gap(10.0)
        .radius(10.0).fill(p.bg).border(0.8,p.line).shadow(8.0,24.0,Color::BLACK.alpha(0.25))
        .child(row().h(32.0).px(9.0).gap(7.0).center_y().radius(6.0).fill(p.card).border(0.5,p.line)
            .child(icon(&SEARCH,13.0).color(p.muted))
            .child(text_input(&s.repo_search,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS"))).key("repo-scope-search").grow(1.0).single_line().color(p.ink)
                .placeholder("Find a repository…",p.muted).label("Find a repository").on_edit(|s:&mut State|&mut s.repo_search)))
        .child(row().gap(7.0).center_y()
            .child(button("repo-select-all","Select all",p,|s|s.set_repo_scope(crate::RepoScope::default())))
            .child(button("repo-deselect-all","Deselect all",p,|s|s.set_repo_scope(crate::RepoScope::Only(Default::default())))))
        .child(rule(p))
        .child(col().key("repo-options-scroll").shrink().min_h(0.0).max_h(300.0).scroll(ScrollSpec{stick_bottom:false}).child(options))
        .child(rule(p))
        .child(row().gap(8.0).center_y()
            .child(col().gap(3.0).grow(1.0).child(label(format!("{selected} of {} selected",available.len()),10.5,500.0,p.secondary))
                .child(label(if matches!(s.repo_scope,crate::RepoScope::AllExcept(_)){"New repositories included"}else{"Explicit selection"},10.0,400.0,p.muted)))
            .child(button("repo-picker-done","Done",p,|s|s.repo_picker_open=false)))
        .child(button("repo-picker-manage","Manage tracked repositories…",p,|s|s.open_repository_preferences()))
        .children(s.scope_error.as_ref().map(|error|label(error,10.0,400.0,c("#d18d6c")).w_full().wrap()))
}
fn repository_settings_page(s:&State,p:Paint)->El {
    let prefs=&s.repo_prefs;let busy=prefs.loading||prefs.saving||prefs.importing;
    let mut rows=col().key("tracked-repo-rows").gap(5.0);
    for (index,repo) in prefs.rows.iter().enumerate(){
        let name=repo.name.clone();let enabled=repo.enabled;
        rows=rows.child(row().key(format!("tracked-repo-{}",repo.name)).gap(9.0).center_y().py(5.0)
            .child(row().key(format!("track-toggle-{}",repo.name)).role("checkbox").label(format!("Track {}",repo.name)).checked(enabled).focusable(true).disabled(busy)
                .size(22.0,29.0).center().on_press(move|s:&mut State|{if !s.repo_prefs.saving&&!s.repo_prefs.loading&&!s.repo_prefs.importing{s.repo_prefs.rows[index].enabled=!s.repo_prefs.rows[index].enabled;s.repo_prefs.dirty=true;}})
                .child(row().size(15.0,15.0).radius(4.0).center().border(0.8,if enabled{p.accent}else{p.muted}).fill(if enabled{p.accent}else{Color::TRANSPARENT})
                    .children(enabled.then(||label("✓",11.0,650.0,if p.dark{p.bg}else{Color::WHITE})))))
            .child(col().grow(1.0).min_w(0.0).gap(3.0)
                .child(label(&repo.name,12.0,500.0,if enabled{p.ink}else{p.muted}).w_full().truncate())
                .child(label(if !enabled{"Tracking disabled".into()}else{s.data.repository_sync.iter().find(|r|r.repo.eq_ignore_ascii_case(&repo.name)).map(|r|r.label(s.data.backend.polling)).unwrap_or_else(||"Save changes to queue import".into())},10.0,400.0,p.secondary).key(format!("repo-sync-{}",repo.name)).w_full().wrap()))
            .child(row().w(240.0).h(32.0).px(8.0).center_y().radius(5.0).fill(p.card).border(0.5,p.line)
                .child(text_input(&repo.include_terms,TextStyle::new(11.0,400.0)).key(format!("track-terms-{}",repo.name)).grow(1.0).single_line().color(p.ink).disabled(busy)
                    .placeholder("All entries, or comma-separated terms",p.muted).label(format!("Include terms for {}",repo.name)).on_edit(move|s:&mut State|&mut s.repo_prefs.rows[index].include_terms)))
            .child(button(&format!("track-remove-{}",repo.name),"Remove",p,move|s|{if !s.repo_prefs.saving&&!s.repo_prefs.loading&&!s.repo_prefs.importing{s.repo_prefs.rows.retain(|row|row.name!=name);s.repo_prefs.dirty=true;}}).disabled(busy)));
    }
    if prefs.rows.is_empty(){rows=rows.child(label(if prefs.loading{"Loading repositories…"}else{"No repositories tracked. Add one below."},12.0,400.0,p.muted).py(22.0));}
    let input=row().h(33.0).px(9.0).gap(8.0).center_y().radius(6.0).fill(p.card).border(0.5,p.line)
        .child(icon(&FOLDER,14.0).color(p.muted))
        .child(text_input(&prefs.add,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS"))).key("new-repository").grow(1.0).single_line().color(p.ink).disabled(busy)
            .placeholder("owner/repo or GitHub repository URL",p.muted).label("Add repository").on_edit(|s:&mut State|&mut s.repo_prefs.add));
    col().key("repository-settings-page").grow(1.0).min_w(0.0).h_full().padding(24.0).gap(14.0)
            .child(label("Repositories",21.0,650.0,p.ink))
            .child(label("Match any include term in the title only, ignoring case. Leave empty to include everything.",12.0,400.0,p.secondary).w_full().wrap())
            .child(rule(p))
            .child(row().gap(9.0).child(label("TRACKING / REPOSITORY",9.5,600.0,p.muted).grow(1.0)).child(label("INCLUDE TERMS",9.5,600.0,p.muted).w(240.0)).child(el().w(66.0)))
            .child(col().key("tracked-repos-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec{stick_bottom:false}).child(rows))
            .child(row().gap(8.0).center_y().child(input.grow(1.0))
                .child(button("add-tracked-repository","Add",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|{let count=s.repo_prefs.rows.len();s.repo_prefs.add();if s.repo_prefs.rows.len()>count{tx.scroll_into_view(format!("tracked-repo-{}",s.repo_prefs.rows.last().unwrap().name));}})))
            .child(label("Removing or disabling a repo hides its entries and stops fetching. Saved data and notes are kept.",11.0,400.0,p.secondary).w_full().wrap())
            .children(prefs.error.as_ref().map(|error|label(error,11.0,500.0,c("#d18d6c")).key("repository-preferences-error").w_full().wrap()))
            .children(prefs.notice.as_ref().map(|notice|label(notice,11.0,400.0,p.accent).key("repository-import-notice").w_full().wrap()))
            .children(prefs.importing.then(||label("Reading repository file…",11.0,500.0,p.accent)))
            .children(prefs.saving.then(||label("Validating access and saving…",11.0,500.0,p.accent).key("repository-preferences-saving")))
            .child(label(&prefs.path,9.5,400.0,p.muted).w_full().truncate().selectable(true))
            .child(rule(p))
            .child(row().gap(8.0).center_y()
                .child(button("import-repository-file","Import file…",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|s.import_repository_file(tx)))
                .child(button("reload-repository-file","Reload file",p,|s|s.reload_repository_preferences()).disabled(busy))
                .child(el().grow(1.0))
                .child(button("cancel-repository-preferences",if s.settings_dirty(){"Cancel"}else{"Close"},p,|s|s.discard_settings()).disabled(prefs.saving||prefs.importing))
                .child(button("save-repository-preferences","Save changes",p,|s|s.save_repository_preferences()).disabled(busy||(!prefs.is_dirty()&&prefs.error.is_none()))))
}
pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark=s.appearance.unwrap_or(cx.system_dark()); let p=paint(dark);
    cx.caption_color(p.secondary);
    if s.auth_blocked() || s.auth.reconnect_open {return github_setup(s,p);}
    if s.settings_window{return settings_content(s,p).children(s.dev.picker.then(||developer_picker(s,p,"settings-developer-profile"))); }
    row().key("window").role("window").w_full().h_full().fill(Color::TRANSPARENT).child(sidebar(s,p)).child(list(s,p)).child(detail(s,p,cx)).children((s.repo_picker_open&&!s.repo_prefs.open).then(||repository_popover(s,p))).children(s.repo_prefs.open.then(||settings_dialog(s,p))).children(s.dev.picker.then(||developer_picker(s,p,if s.repo_prefs.open{"settings-developer-profile"}else{"developer-profile"})))
}

fn maintainer_settings_page(s:&State,p:Paint)->El{
    let prefs=&s.maint_prefs;let busy=prefs.busy();
    let mut rows=col().key("maintainer-rows").gap(5.0);
    for (index,person) in prefs.rows.iter().enumerate(){
        rows=rows.child(row().key(format!("maintainer-{}",person.login)).gap(9.0).py(7.0).center_y()
            .child(avatar(&person.login,25.0,p))
            .child(col().grow(1.0).min_w(0.0).gap(2.0).child(label(&person.login,12.0,500.0,p.ink)).child(label(if person.github_id.is_empty(){"Will resolve GitHub ID on save".into()}else{format!("GitHub ID {}",person.github_id)},10.0,400.0,p.muted)))
            .child(button(&format!("remove-maintainer-{}",person.login),"Remove",p,move|s|{if !s.maint_prefs.busy(){s.maint_prefs.rows.remove(index);s.maint_prefs.dirty=true;}}).disabled(busy)));
    }
    if prefs.rows.is_empty(){rows=rows.child(label(if prefs.loading{"Loading maintainers…"}else{"No maintainers listed. Add a GitHub username below."},12.0,400.0,p.muted).w_full().wrap().py(22.0));}
    col().key("maintainer-settings-page").grow(1.0).min_w(0.0).h_full().padding(24.0).gap(14.0)
        .child(label("Maintainers",21.0,650.0,p.ink))
        .child(label("This roster decides who counts as internal. Entries created by maintainers are excluded from follow-up.",12.0,400.0,p.secondary).w_full().wrap())
        .child(rule(p))
        .child(col().key("maintainers-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec{stick_bottom:false}).child(rows))
        .child(row().gap(8.0).center_y()
            .child(row().grow(1.0).h(33.0).px(9.0).center_y().radius(6.0).fill(p.card).border(0.5,p.line)
                .child(text_input(&prefs.add,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS"))).key("new-maintainer").grow(1.0).single_line().color(p.ink).disabled(busy).placeholder("GitHub username or profile URL",p.muted).label("Add maintainer").on_edit(|s:&mut State|&mut s.maint_prefs.add)))
            .child(button("add-maintainer","Add",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|{let count=s.maint_prefs.rows.len();s.maint_prefs.add();if s.maint_prefs.rows.len()>count{tx.scroll_into_view(format!("maintainer-{}",s.maint_prefs.rows.last().unwrap().login));}})))
        .child(label("Removing a maintainer makes their open entries external again. Saved issues, PRs and notes are kept.",11.0,400.0,p.secondary).w_full().wrap())
        .children(prefs.error.as_ref().map(|error|label(error,11.0,500.0,c("#d18d6c")).key("maintainer-settings-error").w_full().wrap()))
        .children(prefs.notice.as_ref().map(|notice|label(notice,11.0,400.0,p.accent).w_full().wrap()))
        .children(prefs.importing.then(||label("Reading maintainer file…",11.0,500.0,p.accent)))
        .children(prefs.saving.then(||label("Validating users and saving…",11.0,500.0,p.accent)))
        .child(label(&prefs.path,9.5,400.0,p.muted).w_full().truncate().selectable(true))
        .child(rule(p))
        .child(row().gap(8.0).center_y()
            .child(button("import-maintainer-file","Import file…",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|s.import_maintainer_file(tx)))
            .child(button("reload-maintainer-file","Reload file",p,|s|s.reload_maintainers()).disabled(busy))
            .child(el().grow(1.0))
            .child(button("cancel-maintainer-settings",if s.settings_dirty(){"Cancel"}else{"Close"},p,|s|s.discard_settings()).disabled(busy))
            .child(button("save-maintainer-settings","Save changes",p,|s|s.save_maintainers()).disabled(busy||(!prefs.dirty&&prefs.error.is_none()))))
}
fn customer_settings_page(s:&State,p:Paint)->El{
    let prefs=&s.cust_prefs;let busy=prefs.busy();
    let mut rows=col().key("customer-rows").gap(5.0);
    for (index,person) in prefs.rows.iter().enumerate(){
        let login=person.login.clone();let anchor=format!("highlight-customer-{}",person.login);let menu_anchor=anchor.clone();let selected=person.highlight;
        rows=rows.child(col().key(format!("customer-{}",person.login)).w_full().padding(12.0).gap(12.0).radius(8.0).border(0.5,p.line)
            .child(row().w_full().gap(10.0).center_y()
                .child(col().grow(1.0).min_w(0.0).gap(2.0).child(profile(s,&person.login,25.0,12.0,&format!("vip-settings-profile-{}",person.login),p)).child(label(if person.github_id.is_empty(){"Will resolve GitHub ID on save".into()}else{format!("GitHub ID {}",person.github_id)},10.0,400.0,p.muted)))
                .child(button(&anchor,&format!("{}  ▾",person.highlight.label()),p,|_|{}).w(148.0).disabled(busy).on_press_tx(move |_:&mut State,tx|{
                    use tracker_core::customer_config::Highlight;
                    let login=login.clone();
                    tx.popup_menu(rune::PopupMenu::picker(Highlight::ALL.map(|h|(h.key(),h.label())),selected.key()),menu_anchor.clone(),move |s:&mut State,picked,_|{
                        if s.cust_prefs.busy(){return;}
                        if let Some(style)=Highlight::ALL.into_iter().find(|h|Some(h.key())==picked) {
                            if let Some(person)=s.cust_prefs.rows.iter_mut().find(|p|p.login==login){person.highlight=style;s.cust_prefs.dirty=true;}
                        }
                    });
                }))
                .child(button(&format!("remove-customer-{}",person.login),"Remove",p,move|s|{if !s.cust_prefs.busy(){s.cust_prefs.rows.remove(index);s.cust_prefs.names.remove(index);s.cust_prefs.notes.remove(index);s.cust_prefs.companies.remove(index);s.cust_prefs.dirty=true;}}).w(70.0).disabled(busy)))
            .child(row().w_full().gap(12.0)
                .child(col().w(0.0).grow(1.0).min_w(0.0).gap(5.0).child(label("Full name (optional)",10.5,500.0,p.secondary))
                    .child(row().h(32.0).px(9.0).center_y().radius(5.0).fill(p.card).border(0.5,p.line).child(text_input(&prefs.names[index],TextStyle::new(12.0,400.0)).key(format!("customer-name-{index}")).w_full().single_line().color(p.ink).disabled(busy).placeholder("Full name",p.muted).label("Full name").on_edit(move|s:&mut State|{&mut s.cust_prefs.names[index]}))))
                .child(col().w(0.0).grow(1.0).min_w(0.0).gap(5.0).child(label("Company (optional)",10.5,500.0,p.secondary))
                    .child(row().h(32.0).px(9.0).center_y().radius(5.0).fill(p.card).border(0.5,p.line).child(text_input(&prefs.companies[index],TextStyle::new(12.0,400.0)).key(format!("vip-company-{index}")).w_full().single_line().color(p.ink).disabled(busy).placeholder("Company or organization",p.muted).label("Company").on_edit(move|s:&mut State|{&mut s.cust_prefs.companies[index]})))))
            .child(col().w_full().gap(5.0).child(label("Notes (optional)",10.5,500.0,p.secondary))
                .child(col().w_full().padding(9.0).radius(5.0).fill(p.card).border(0.5,p.line).child(text_input(&prefs.notes[index],TextStyle::new(12.0,400.0)).key(format!("vip-notes-{index}")).h(52.0).w_full().color(p.ink).disabled(busy).placeholder("Context, relationship, or follow-up reminders",p.muted).label("VIP notes").on_edit(move|s:&mut State|{&mut s.cust_prefs.notes[index]})))));
    }
    if prefs.rows.is_empty(){rows=rows.child(label(if prefs.loading{"Loading VIPs…"}else{"No VIPs listed. Add a GitHub username below."},12.0,400.0,p.muted).w_full().wrap().py(22.0));}
    col().key("customer-settings-page").grow(1.0).min_w(0.0).h_full().padding(24.0).gap(14.0)
        .child(label("VIPs",21.0,650.0,p.ink))
        .child(label("Highlight issues and PRs opened by VIPs. Choose a highlight for each profile.",12.0,400.0,p.secondary).w_full().wrap())
        .child(rule(p))
        .child(col().key("customers-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec{stick_bottom:false}).child(rows))
        .child(row().gap(8.0).center_y()
            .child(row().grow(1.0).h(33.0).px(9.0).center_y().radius(6.0).fill(p.card).border(0.5,p.line)
                .child(text_input(&prefs.add,TextStyle::new(12.0,400.0).family(Family::Named(".SF NS"))).key("new-customer").grow(1.0).single_line().color(p.ink).disabled(busy).placeholder("GitHub username or profile URL",p.muted).label("Add VIP").on_edit(|s:&mut State|&mut s.cust_prefs.add)))
            .child(button("add-customer","Add",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|{let count=s.cust_prefs.rows.len();s.cust_prefs.add();if s.cust_prefs.rows.len()>count{tx.scroll_into_view(format!("customer-{}",s.cust_prefs.rows.last().unwrap().login));}})))
        .child(label("Import JSON with login and optional fullName, company, and notes. Choose a highlight from the dropdown.",11.0,400.0,p.secondary).w_full().wrap())
        .children(prefs.error.as_ref().map(|error|label(error,11.0,500.0,c("#d18d6c")).key("customer-settings-error").w_full().wrap()))
        .children(prefs.notice.as_ref().map(|notice|label(notice,11.0,400.0,p.accent).w_full().wrap()))
        .children(prefs.importing.then(||label("Reading VIP file…",11.0,500.0,p.accent)))
        .children(prefs.saving.then(||label("Validating users and saving…",11.0,500.0,p.accent)))
        .child(label(&prefs.path,9.5,400.0,p.muted).w_full().truncate().selectable(true))
        .child(rule(p))
        .child(row().gap(8.0).center_y()
            .child(button("import-customer-file","Import file…",p,|_|{}).disabled(busy).on_press_tx(|s:&mut State,tx|s.import_customer_file(tx)))
            .child(button("reload-customer-file","Reload file",p,|s|s.reload_customers()).disabled(busy))
            .child(el().grow(1.0))
            .child(button("cancel-customer-settings",if s.settings_dirty(){"Cancel"}else{"Close"},p,|s|s.discard_settings()).disabled(busy))
            .child(button("save-customer-settings","Save changes",p,|s|s.save_customers()).disabled(busy||(!prefs.is_dirty()&&prefs.error.is_none()))))
}
fn settings_close_confirmation(s:&State,p:Paint)->Option<El>{
    s.settings_close_confirm.then(||row().key("discard-settings-backdrop").absolute(0.0,0.0).w_full().h_full().center().fill(Color::BLACK.alpha(0.35))
        .child(col().key("discard-settings-dialog").role("dialog").label("Discard unsaved changes?").modal(true,|s:&mut State|s.settings_close_confirm=false).w(400.0).padding(24.0).gap(16.0).radius(12.0).fill(p.card).border(0.5,p.line)
            .child(label("Discard unsaved changes?",18.0,600.0,p.ink))
            .child(label("Changes in Settings have not been saved.",13.0,400.0,p.secondary).wrap())
            .child(row().gap(10.0).child(button("keep-editing-settings","Keep editing",p,|s|s.settings_close_confirm=false)).child(button("discard-settings","Discard changes",p,|s|s.discard_settings())))))
}
fn workspace_settings(s:&State,p:Paint)->El {
    let busy=s.settings_busy();
    let mut content=col().gap(15.0).w_full()
        .child(label("Workspace",22.0,650.0,p.ink))
        .child(label("Import one workspace.json file containing repositories, maintainers, and VIPs.",13.0,400.0,p.secondary).w_full().wrap())
        .child(button("import-workspace","Import workspace…",p,|_|{}).w(190.0).disabled(!s.workspace_ready()).on_press_tx(|s:&mut State,tx|s.import_workspace(tx)));
    if s.workspace.reading{content=content.child(label("Reading workspace…",12.0,400.0,p.secondary));}
    if let Some(bundle)=&s.workspace.draft {
        content=content.child(label("Review before saving",16.0,600.0,p.ink))
            .child(label("Saving replaces all three lists, including any unsaved edits. An empty list clears that section. Your token and notes on issues stay unchanged.",12.0,400.0,p.secondary).w_full().wrap());
        for (title,values) in [
            ("Repositories",bundle.repositories.iter().map(|r|format!("{}{}{}",r.name,if r.enabled{""}else{" (disabled)"},if r.include_terms.is_empty(){String::new()}else{format!(" · titles: {}",r.include_terms.join(", "))})).collect::<Vec<_>>()),
            ("Maintainers",bundle.maintainers.iter().map(|m|m.login.clone()).collect()),
            ("VIPs",bundle.vips.iter().map(|v|format!("{} · {}{}",v.login,v.highlight.label(),if v.company.is_empty(){String::new()}else{format!(" · {}",v.company)})).collect()),
        ] {
            content=content.child(col().padding(12.0).gap(8.0).w_full().radius(7.0).fill(p.card)
                .child(label(format!("{title} · {}",values.len()),13.0,600.0,p.ink))
                .child(label(if values.is_empty(){"This list will be cleared.".into()}else{values.join("\n")},12.0,400.0,p.secondary).w_full().wrap()));
        }
    } else {
        content=content.child(label("Share this file through your team's private configuration repository. Importing copies the lists into this app; pull and import again to pick up team changes.",12.0,400.0,p.secondary).w_full().wrap());
    }
    if let Some(error)=&s.workspace.error{content=content.child(label(error,12.0,400.0,c("#d18d6c")).w_full().wrap());}
    for (name,error) in [("Repositories",&s.repo_prefs.error),("Maintainers",&s.maint_prefs.error),("VIPs",&s.cust_prefs.error)] {
        if let Some(error)=error{content=content.child(label(format!("{name}: {error}"),12.0,400.0,c("#d18d6c")).w_full().wrap());}
    }
    if let Some(notice)=&s.workspace.notice{content=content.child(label(notice,12.0,400.0,p.green).w_full().wrap());}
    col().key("workspace-settings").w(0.0).grow(1.0).min_w(0.0).h_full().padding(24.0).gap(16.0)
        .child(col().key("workspace-review-scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec{stick_bottom:false}).child(content))
        .child(row().gap(8.0).child(el().grow(1.0))
            .child(button("cancel-workspace",if s.settings_dirty(){"Cancel"}else{"Close"},p,|s|s.discard_settings()).disabled(busy))
            .child(button("save-workspace",if s.workspace.applying{"Saving…"}else{"Save workspace"},p,|s|s.apply_workspace()).disabled(!s.workspace_ready()||s.workspace.draft.is_none())))
}
fn settings_content(s:&State,p:Paint)->El{
    let mut navigation=col().w(170.0).h_full().padding(14.0).gap(6.0).fill(p.side).border_sides([0.0,0.5,0.0,0.0],p.line);
    for (tab,title) in [("workspace","Workspace"),("repositories","Repositories"),("maintainers","Maintainers"),("customers","VIPs"),("developer","Developer work")]{
        navigation=navigation.child(row().key(format!("settings-{tab}")).role("button").label(title).focusable(true).h(34.0).px(10.0).center_y().radius(7.0).fill(if s.settings_tab==tab{p.selected}else{Color::TRANSPARENT}).hover(p.hover).disabled(s.settings_busy()).on_press(move|s:&mut State|s.settings_page(tab)).child(label(title,12.0,550.0,if s.settings_tab==tab{p.accent}else{p.secondary})));
    }
    col().key("settings-content").w_full().h_full().fill(p.bg)
        .child(row().h(60.0).pl(if s.settings_window{90.0}else{24.0}).center_y().window_drag().border_sides([0.0,0.0,0.5,0.0],p.line).child(label("Settings",16.0,600.0,p.ink)))
        .child(row().grow(1.0).min_h(0.0).child(navigation).child(if s.settings_tab=="workspace"{workspace_settings(s,p)}else if s.settings_tab=="developer"{developer_settings(s,p)}else if s.settings_tab=="customers"{customer_settings_page(s,p)}else if s.settings_tab=="maintainers"{maintainer_settings_page(s,p)}else{repository_settings_page(s,p)}))
        .children(settings_close_confirmation(s,p))
}
fn settings_dialog(s:&State,p:Paint)->El{
    row().key("repository-preferences-backdrop").transition(rune::Transition::none()).instant_changes().motion_intent(rune::MotionIntent::CUT).absolute(0.0,0.0).w_full().h_full().center().fill(Color::BLACK.alpha(0.28))
        .child(col().key("repository-preferences").role("dialog").label("Settings").modal(false,|s:&mut State|s.request_settings_close()).transition(rune::Transition::none()).instant_changes().motion_intent(rune::MotionIntent::CUT).w(920.0).h(680.0).radius(13.0).clip_rounded(13.0).fill(p.bg).border(0.8,p.line).shadow(12.0,35.0,Color::BLACK.alpha(0.25)).child(settings_content(s,p)))
}

fn automatic_sync_status(s:&State,p:Paint)->El {
    let repos:Vec<_>=s.data.repository_sync.iter().filter(|r|s.repo_scope.includes(&r.repo)).collect();
    let important=repos.iter().filter(|r|r.status!="ready").count();
    let errors=repos.iter().filter(|r|r.status=="error").count();
    let summary=if !s.data.backend.polling {"Automatic polling paused".into()}
        else if repos.is_empty(){"Waiting for repository status…".into()}
        else if important==0{format!("All {} repositories up to date",repos.len())}
        else if errors>0{format!("{errors} repositories need attention")}
        else{format!("Syncing {important} of {} repositories",repos.len())};
    let mut status=col().key("automatic-sync-status").h(84.0).px(18.0).pb(10.0).gap(4.0)
        .child(row().gap(6.0).center_y().child(label(summary,10.5,500.0,p.secondary).grow(1.0).min_w(0.0).truncate())
            .child(button("view-sync-settings","Details",p,|s|{s.open_repository_preferences();s.settings_page("repositories");if let Some((app,_))=super::SETTINGS_WINDOW.get(){app.update(|s,_|s.settings_page("repositories"));}})));
    if s.data.backend.polling {
        for repo in repos.iter().filter(|r|r.status!="ready").take(2){
            status=status.child(label(format!("{} · {}",&repo.repo,repo.label(true)),10.0,400.0,if repo.status=="error"{c("#d18d6c")}else{p.accent}).w_full().truncate());
        }
    }
    status
}

fn welcome(s:&State,p:Paint)->El {
    col().key("welcome-screen").w_full().h_full().fill(p.bg)
        .child(row().h(60.0).w_full().window_drag())
        .child(col().grow(1.0).w_full().center()
            .child(col().w(480.0).gap(20.0).center()
                .child(icon(&THREADKEEPER,88.0).label("Threadkeeper logo"))
                .child(label("Threadkeeper",30.0,650.0,p.ink))
                .child(label("Keep the conversation moving.",14.0,400.0,p.secondary))
                .child(el().h(16.0))
                .child(button("get-started","Get started",p,|_|{}).on_press_tx(|s:&mut State,tx|{s.auth.started=true;tx.instant();}).w(180.0).h(40.0).disabled(s.auth.checking))
                .child(col().gap(4.0).center().child(label("Follow GitHub issues and pull requests across",12.0,400.0,p.secondary)).child(label("your repositories and integrations.",12.0,400.0,p.secondary)))
                .child(label("Connect a read-only GitHub token to begin.",12.0,400.0,p.secondary))
                .children(s.auth.checking.then(||label("Checking for a saved token…",11.0,400.0,p.muted)))))
        .child(row().h(60.0).w_full().center().child(label(concat!("Version ",env!("CARGO_PKG_VERSION")),11.0,400.0,p.muted)))
}

fn github_setup(s:&State,p:Paint)->El {
    if !s.auth.started {return welcome(s,p);}

    let busy=s.auth.busy();
    col().key("github-setup-screen").w_full().h_full().fill(p.bg)
        .child(row().h(60.0).w_full().window_drag())
        .child(col().grow(1.0).w_full().center()
            .child(col().key("github-setup-dialog").role("dialog").label("Connect GitHub to continue").modal(true,|s:&mut State|{if s.auth.disconnected{s.auth.reconnect_open=false;}})
                .w(600.0).padding(30.0).gap(16.0)
                .child(row().gap(12.0).center_y()
                    .child(icon(&THREADKEEPER,40.0).label("Threadkeeper logo"))
                    .child(col().gap(3.0).child(label("Threadkeeper",20.0,650.0,p.ink)).child(label("Keep threads moving.",11.0,400.0,p.secondary))))
                .child(label("Connect GitHub",24.0,650.0,p.ink))
                .child(label(if s.auth.disconnected {"Reconnect to resume syncing. You can close this screen and keep using your saved data."} else {"Connect your account to start tracking issues and pull requests. Complete setup or quit the app."},13.0,400.0,p.secondary).w_full().wrap())
                .child(label("1. Create a fine-grained personal access token on GitHub. Give it a name and an expiration date.",12.0,400.0,p.ink).w_full().wrap())
                .child(button("create-github-token","Create token on GitHub ↗",p,|_|{}).disabled(busy||crate::auth::demo_requested()).on_press_tx(|s:&mut State,tx|{if let Err(_)=tx.open_url(crate::auth::TOKEN_URL){s.auth.error=Some("Could not open GitHub in your browser.".into());}}))
                .child(label("2. Choose Public repositories. Public read access is included; no extra permissions or write access are needed.",12.0,400.0,p.ink).w_full().wrap())
                .child(label("For private repositories, select those repositories and grant Issues and Pull requests read-only access (Metadata is included). Your organization may need to approve it.",11.0,400.0,p.secondary).w_full().wrap())
                .child(label("3. Generate the token, copy it, and paste it below.",12.0,400.0,p.ink).w_full().wrap())
                .child(row().h(40.0).px(10.0).center_y().radius(6.0).fill(p.card).border(0.8,p.line)
                    .child(text_input(&s.auth.token,TextStyle::new(13.0,400.0)).key("github-token-input").label("GitHub personal access token").secure().single_line().grow(1.0).min_w(0.0).clip_soft(0.0).color(p.ink).placeholder("Paste your token",p.muted).disabled(busy).on_edit(|s:&mut State|&mut s.auth.token).on_submit(|s:&mut State,tx|{s.connect_github();if s.demo{tx.resize_window((1380.0,880.0));tx.center_window();}})))
                .child(label("Saved locally in a private credentials file, readable only by your macOS user.",11.0,400.0,p.secondary).w_full().wrap())
                .children(s.auth.error.as_ref().map(|error|label(error,12.0,500.0,c("#d18d6c")).key("github-setup-error").w_full().wrap()))
                .children(busy.then(||label(if s.auth.checking{"Checking for an existing token…"}else{"Validating token and saving locally…"},12.0,500.0,p.accent)))
                .child(row().gap(8.0).center_y()
                    .child(button("quit-github-setup",if s.auth.disconnected {"Close"}else{"Quit app"},p,|_|{}).on_press_tx(|s:&mut State,tx|{if s.auth.disconnected{s.auth.reconnect_open=false;tx.instant();}else{tx.quit();}}))
                    .children((!s.auth.disconnected).then(||button("retry-github-setup","Retry saved token",p,|s|s.auth.discover()).disabled(busy||crate::auth::demo_requested())))
                    .child(el().grow(1.0))
                    .child(button("connect-github-token","Connect & continue",p,|_|{}).on_press_tx(|s:&mut State,tx|{s.connect_github();if s.demo{tx.resize_window((1380.0,880.0));tx.center_window();}}).disabled(busy||s.auth.token.text.trim().is_empty())))))
}

fn developer_profile(s:&State,p:Paint,key:&str)->El {
    col().gap(7.0).child(label("GITHUB PROFILE",9.5,600.0,p.muted))
        .child(button(key,&format!("{}  ▾",if s.dev.login.is_empty(){"Choose a maintainer"}else{&s.dev.login}),p,|s|{s.dev.picker=!s.dev.picker;if !s.preview{s.dev.profiles_result.request((),|_|data::maintainer_settings());}}).w_full().h(34.0))
}
fn developer_picker(s:&State,p:Paint,anchor:&str)->El {
    let mut options=col().gap(4.0);
    for login in &s.dev.profiles {let value=login.clone();options=options.child(button(&format!("developer-select-{login}"),login,p,move|s|s.choose_developer(value.clone())).w_full());}
    if s.dev.profiles.is_empty(){options=options.child(label("Add profiles in Settings → Maintainers.",12.0,400.0,p.secondary).w_full().wrap());}
    col().key("developer-profile-picker").role("dialog").label("Choose developer").focus_return(anchor).modal(true,|s:&mut State|s.dev.picker=false)
        .anchored_fit(anchor,rune::Side::Bottom,rune::Align::Start,6.0).w(330.0).max_h(340.0).padding(10.0).radius(8.0).fill(p.side).border(0.5,p.line)
        .child(col().min_h(0.0).max_h(300.0).scroll(ScrollSpec{stick_bottom:false}).child(options))
}
fn developer_settings(s:&State,p:Paint)->El {
    col().key("developer-settings-page").grow(1.0).min_w(0.0).padding(24.0).gap(18.0)
        .child(label("Developer work",20.0,650.0,p.ink))
        .child(label("Choose a maintainer to follow their reviews, assignments, authored work, and conversations across enabled repositories.",12.0,400.0,p.secondary).w_full().wrap())
        .child(developer_profile(s,p,"settings-developer-profile").w(330.0).max_w(330.0))
        .child(label("Selection is saved immediately. Manage available profiles under Maintainers. Follow-up title filters do not apply here.",11.5,400.0,p.secondary).w_full().wrap())
        .children(s.dev.error.as_ref().map(|e|label(e,11.0,400.0,p.secondary).w_full().wrap()))
}
fn developer_status(s:&State,p:Paint)->El {
    col().px(18.0).pb(12.0).gap(5.0)
        .child(label(if s.dev.paused&&s.dev.retry_at.is_some_and(|at|at>chrono::Utc::now().timestamp_millis()){format!("GitHub paused until {}",chrono::DateTime::from_timestamp_millis(s.dev.retry_at.unwrap()).map(|at|at.with_timezone(&chrono::Local).format("%H:%M:%S").to_string()).unwrap_or_default())}else if s.dev.loading{s.data.backend.developer_progress.clone().unwrap_or_else(||"Loading developer work · using shared sync".into())}else if let Some(at)=s.dev.checked{format!("Last complete check · {}",chrono::DateTime::from_timestamp(at,0).map(|at|at.format("%b %d %H:%M UTC").to_string()).unwrap_or_default())}else{if s.dev.login.is_empty(){"Choose a maintainer to load developer work".into()}else{format!("{} · queue not fully loaded yet",s.dev.login)}},10.5,400.0,p.secondary).w_full().wrap())
        .children(s.dev.error.as_ref().map(|e|col().w_full().max_h(80.0).min_h(0.0).scroll(ScrollSpec{stick_bottom:false}).child(label(e,10.5,400.0,c("#d18d6c")).w_full().wrap())))
}

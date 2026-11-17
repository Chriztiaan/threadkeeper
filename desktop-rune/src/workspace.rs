//! One portable workspace file, reviewed before replacing local configuration.
use crate::{State, data};
use rune::{Latest, Tx};
use tracker_core::{config::{Config, Repository, Maintainer},customer_config::Customer};

#[derive(Clone,serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub repositories:Vec<Repository>,
    pub maintainers:Vec<Maintainer>,
    pub vips:Vec<Customer>,
}
impl Bundle {
    pub fn parse(raw:&str)->Result<Self,String> {
        let parse=||->Result<Self,Box<dyn std::error::Error>>{
            let file:Self=serde_json::from_str(raw)?;
            let base=Config{interval:300,maintainers_file:"maintainers.json".into(),repositories:vec![],maintainers:vec![],people:vec![]};
            Ok(Self{
                repositories:tracker_core::repository_config::validated(file.repositories,&base)?.repositories,
                maintainers:tracker_core::maintainer_config::validated(file.maintainers,false)?,
                vips:tracker_core::customer_config::validated(file.vips,false)?,
            })
        };
        parse().map_err(|e|format!("Could not import workspace: {e}"))
    }
}
#[derive(Default)]
pub struct Import {
    pub result:Latest<Result<Bundle,String>>,
    pub draft:Option<Bundle>,
    pub reading:bool,
    pub applying:bool,
    pub error:Option<String>,
    pub notice:Option<String>,
}
impl State {
    pub(crate) fn workspace_ready(&self)->bool {
        !self.settings_busy()&&!self.repo_prefs.loading&&self.maint_prefs.loaded&&self.cust_prefs.loaded
            && !self.maint_prefs.loading&&!self.cust_prefs.loading
    }
    pub(crate) fn import_workspace(&mut self,tx:&mut Tx) {
        if !self.workspace_ready(){return;}
        tx.open_file(rune::FileDialogOptions{title:"Import workspace".into(),..Default::default()}.filter("Workspace JSON",&["json"]),|s:&mut State,result,_|match result {
            Ok(Some(path))=>{
                s.workspace.reading=true;s.workspace.error=None;s.workspace.notice=None;
                s.workspace.result.request(path,|path|std::fs::read_to_string(path).map_err(|e|format!("Could not read workspace: {e}")).and_then(|raw|Bundle::parse(&raw)));
            },
            Ok(None)=>{},Err(error)=>s.workspace.error=Some(error.to_string()),
        });
    }
    pub(crate) fn apply_workspace(&mut self) {
        if !self.workspace_ready(){return;}
        let Some(bundle)=self.workspace.draft.clone() else{return;};
        self.repo_prefs.import(bundle.repositories);
        self.maint_prefs.import(bundle.maintainers);
        self.cust_prefs.import(bundle.vips);
        self.workspace.error=None;self.workspace.notice=None;self.workspace.applying=true;
        self.save_repository_preferences();self.save_maintainers();self.save_customers();
    }
    pub(crate) fn poll_workspace(&mut self)->bool {
        let mut changed=false;
        if let Some(result)=self.workspace.result.take(){
            self.workspace.reading=false;
            match result {Ok(bundle)=>{self.workspace.draft=Some(bundle);self.workspace.error=None;},Err(error)=>{self.workspace.draft=None;self.workspace.error=Some(error);}}
            changed=true;
        }
        if self.workspace.applying&&!self.repo_prefs.saving&&!self.maint_prefs.saving&&!self.cust_prefs.saving {
            self.workspace.applying=false;
            let errors=[("Repositories",&self.repo_prefs.error),("Maintainers",&self.maint_prefs.error),("VIPs",&self.cust_prefs.error)].into_iter().filter_map(|(name,e)|e.as_ref().map(|e|format!("{name}: {e}"))).collect::<Vec<_>>();
            if errors.is_empty(){self.workspace.draft=None;self.workspace.notice=Some("Workspace saved. Repositories, maintainers, and VIPs are up to date.".into());}
            else{self.workspace.error=Some(format!("Some lists could not be saved. Successful changes were kept. Review the errors and retry.\n{}",errors.join("\n")));}
            if self.settings_window && let Some(app)=crate::MAIN_APP.get(){app.update(|s,_|{s.refresh();s.load_detail();s.load_developer_cache();});}
            if !self.preview{self.refresh();self.dev.profiles_result.request((),|_|data::maintainer_settings());}
            changed=true;
        }
        changed
    }
}

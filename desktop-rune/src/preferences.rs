use rune::TextEdit;
use tracker_core::{
    config::Repository,
    repository_config::{RepositorySettings, normalize_name},
};

pub struct Row {
    pub name: String,
    pub include_terms: TextEdit,
    pub initial_terms: String,
    pub enabled: bool,
}
#[derive(Default)]
pub struct RepositoryPreferences {
    pub open: bool,
    pub rows: Vec<Row>,
    pub revision: String,
    pub path: String,
    pub error: Option<String>,
    pub loading: bool,
    pub saving: bool,
    pub importing: bool,
    pub notice: Option<String>,
    pub dirty: bool,
    pub add: TextEdit,
}
impl RepositoryPreferences {
    pub fn is_dirty(&self)->bool {self.dirty || self.rows.iter().any(|r| r.include_terms.text != r.initial_terms)}
    pub fn load(&mut self, settings: RepositorySettings) {
        self.rows = settings
            .repositories
            .into_iter()
            .map(|r| Row {
                name: r.name,
                include_terms: TextEdit::new(&r.include_terms.join(", ")),
                initial_terms: r.include_terms.join(", "),
                enabled: r.enabled,
            })
            .collect();
        self.revision = settings.revision;
        self.path = settings.path;
        self.error = settings.error;
        self.loading = false;
        self.saving = false;
        self.importing = false;
        self.notice = None;
        self.dirty = false;
        self.add = TextEdit::new("");
    }
    pub fn repositories(&self) -> Vec<Repository> {
        self.rows
            .iter()
            .map(|r| Repository {
                name: r.name.clone(),
                include_terms: r.include_terms.text.split(',').map(str::to_owned).collect(),
                enabled: r.enabled,
            })
            .collect()
    }
    pub fn add(&mut self) {
        if self.saving || self.loading || self.importing {
            return;
        }
        match normalize_name(&self.add.text) {
            Ok(name) => {
                if self.rows.iter().any(|r| r.name == name) {
                    self.error = Some("That repository is already listed.".into());
                    return;
                }
                self.rows.push(Row {
                    name,
                    include_terms: TextEdit::new(""),
                    initial_terms: String::new(),
                    enabled: true,
                });
                self.add = TextEdit::new("");
                self.dirty = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

pub fn read_import(path: std::path::PathBuf) -> Result<Vec<Repository>, String> {
    let read = || -> Result<Vec<Repository>, Box<dyn std::error::Error>> {
        let value: serde_json::Value = serde_json::from_reader(std::fs::File::open(path)?)?;
        let repositories: Vec<Repository> = serde_json::from_value(
            value
                .get("repositories")
                .ok_or("Expected a JSON object with a repositories array")?
                .clone(),
        )?;
        let base = tracker_core::config::Config {
            interval: 300,
            maintainers_file: "maintainers.json".into(),
            repositories: vec![],
            maintainers: vec![],
            people: vec![],
        };
        Ok(tracker_core::repository_config::validated(repositories, &base)?.repositories)
    };
    read().map_err(|error| format!("Could not import repositories: {error}"))
}
impl RepositoryPreferences {
    pub fn import(&mut self, repositories: Vec<Repository>) {
        let count = repositories.len();
        self.rows = repositories
            .into_iter()
            .map(|repo| Row {
                name: repo.name,
                include_terms: TextEdit::new(&repo.include_terms.join(", ")),
                initial_terms: repo.include_terms.join(", "),
                enabled: repo.enabled,
            })
            .collect();
        self.importing = false;
        self.dirty = true;
        self.error = None;
        self.add = TextEdit::new("");
        self.notice = Some(format!(
            "Imported {count} repositories. Review the list and save changes to apply it."
        ));
    }
}

#[derive(Default)]
pub struct MaintainerPreferences {
    pub rows: Vec<tracker_core::config::Maintainer>, pub revision:String, pub path:String,
    pub error:Option<String>,pub notice:Option<String>,pub loading:bool,pub saving:bool,pub importing:bool,pub dirty:bool,pub loaded:bool,pub add:TextEdit,
}
impl MaintainerPreferences{
    pub fn busy(&self)->bool{self.loading||self.saving||self.importing}
    pub fn load(&mut self,settings:tracker_core::maintainer_config::MaintainerSettings){
        self.rows=settings.maintainers;self.revision=settings.revision;self.path=settings.path;self.error=settings.error;
        self.loading=false;self.saving=false;self.importing=false;self.dirty=false;self.loaded=true;self.notice=None;self.add=TextEdit::new("");
    }
    pub fn add(&mut self){
        if self.busy(){return;}
        match tracker_core::maintainer_config::normalize_login(&self.add.text){
            Ok(login)=>{if self.rows.iter().any(|m|m.login.eq_ignore_ascii_case(&login)){self.error=Some("That maintainer is already listed.".into());return;}
                self.rows.push(tracker_core::config::Maintainer{login,github_id:String::new()});self.add=TextEdit::new("");self.error=None;self.dirty=true;},
            Err(error)=>self.error=Some(error.to_string()),
        }
    }
    pub fn import(&mut self,rows:Vec<tracker_core::config::Maintainer>){let count=rows.len();self.rows=rows;self.importing=false;self.dirty=true;self.error=None;self.add=TextEdit::new("");self.notice=Some(format!("Imported {count} maintainers. Review and save changes to apply the roster."));}
}
pub fn read_maintainer_import(path:std::path::PathBuf)->Result<Vec<tracker_core::config::Maintainer>,String>{
    let result=(||->Result<Vec<tracker_core::config::Maintainer>,Box<dyn std::error::Error>>{
        let file:tracker_core::maintainer_config::MaintainerFile=serde_json::from_reader(std::fs::File::open(path)?)?;
        Ok(tracker_core::maintainer_config::validated(file.maintainers,false)?)
    })();result.map_err(|e|format!("Could not import maintainers: {e}"))
}

#[derive(Default)]
pub struct CustomerPreferences {
    pub names:Vec<TextEdit>, pub notes:Vec<TextEdit>, pub companies:Vec<TextEdit>,
    pub rows: Vec<tracker_core::customer_config::Customer>, pub revision:String, pub path:String,
    pub error:Option<String>,pub notice:Option<String>,pub loading:bool,pub saving:bool,pub importing:bool,pub dirty:bool,pub loaded:bool,pub add:TextEdit,
}
impl CustomerPreferences{
    pub fn is_dirty(&self)->bool {
        self.dirty || self.rows.iter().enumerate().any(|(i,p)|
            self.names[i].text.trim()!=p.full_name || self.companies[i].text.trim()!=p.company || self.notes[i].text.trim()!=p.notes)
    }

    pub fn customers(&self)->Vec<tracker_core::customer_config::Customer>{self.rows.iter().enumerate().map(|(i,p)|{let mut p=p.clone();p.full_name=self.names[i].text.trim().into();p.notes=self.notes[i].text.trim().into();p.company=self.companies[i].text.trim().into();p}).collect()}
    pub fn busy(&self)->bool{self.loading||self.saving||self.importing}
    pub fn load(&mut self,settings:tracker_core::customer_config::CustomerSettings){
        self.companies=settings.customers.iter().map(|p|TextEdit::new(&p.company)).collect();self.names=settings.customers.iter().map(|p|TextEdit::new(&p.full_name)).collect();self.notes=settings.customers.iter().map(|p|TextEdit::new(&p.notes)).collect();self.rows=settings.customers;self.revision=settings.revision;self.path=settings.path;self.error=settings.error;
        self.loading=false;self.saving=false;self.importing=false;self.dirty=false;self.loaded=true;self.notice=None;self.add=TextEdit::new("");
    }
    pub fn add(&mut self){
        if self.busy(){return;}
        match tracker_core::customer_config::normalize_login(&self.add.text){
            Ok(login)=>{if self.rows.iter().any(|m|m.login.eq_ignore_ascii_case(&login)){self.error=Some("That VIP is already listed.".into());return;}
                self.companies.push(TextEdit::new(""));self.names.push(TextEdit::new(""));self.notes.push(TextEdit::new(""));self.rows.push(tracker_core::customer_config::Customer{login,github_id:String::new(),..Default::default()});self.add=TextEdit::new("");self.error=None;self.dirty=true;},
            Err(error)=>self.error=Some(error.to_string()),
        }
    }
    pub fn import(&mut self,rows:Vec<tracker_core::customer_config::Customer>){let count=rows.len();self.companies=rows.iter().map(|p|TextEdit::new(&p.company)).collect();self.names=rows.iter().map(|p|TextEdit::new(&p.full_name)).collect();self.notes=rows.iter().map(|p|TextEdit::new(&p.notes)).collect();self.rows=rows;self.importing=false;self.dirty=true;self.error=None;self.add=TextEdit::new("");self.notice=Some(format!("Imported {count} VIPs. Review and save changes to apply the roster."));}
}
pub fn read_customer_import(path:std::path::PathBuf)->Result<Vec<tracker_core::customer_config::Customer>,String>{
    let result=(||->Result<Vec<tracker_core::customer_config::Customer>,Box<dyn std::error::Error>>{
        let file:tracker_core::customer_config::CustomerFile=serde_json::from_reader(std::fs::File::open(path)?)?;
        Ok(tracker_core::customer_config::validated(file.customers,false)?)
    })();result.map_err(|e|format!("Could not import VIPs: {e}"))
}

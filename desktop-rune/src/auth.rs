use rune::{Latest, TextEdit};

pub const TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new?name=Threadkeeper&description=Read%20public%20issues%20and%20pull%20requests&expires_in=90";

pub struct Setup {
    pub ready: bool,
    pub started: bool,
    pub disconnected: bool,
    pub reconnect_open: bool,
    pub checking: bool,
    pub saving: bool,
    pub token: TextEdit,
    pub error: Option<String>,
    pub discovered: Latest<Result<Option<String>, String>>,
    pub connected: Latest<Result<String, String>>,
}
impl Default for Setup {
    fn default() -> Self {
        Self {
            ready: false,
            started: false,
            disconnected: false,
            reconnect_open: false,
            checking: false,
            saving: false,
            token: TextEdit::new("").secure(),
            error: None,
            discovered: Latest::default(),
            connected: Latest::default(),
        }
    }
}
impl Setup {
    pub fn busy(&self) -> bool {
        self.checking || self.saving
    }
    pub fn discover(&mut self) {
        if self.busy() {
            return;
        }
        self.checking = true;
        self.error = None;
        self.discovered.request((), |_| discover());
    }
    pub fn connect(&mut self) {
        if self.busy() {
            return;
        }
        let token = self.token.text.trim().to_owned();
        if token.is_empty() {
            self.error = Some("Paste your GitHub token to continue.".into());
            return;
        }
        self.saving = true;
        self.error = None;
        self.connected.request(token, connect);
    }
}

pub fn demo_requested()->bool {std::env::var("THREADKEEPER_DEMO").is_ok_and(|v|v=="1") || std::env::var("GITHUB_TOKEN").is_ok_and(|v|v.trim()=="mock_token")}

pub fn discover() -> Result<Option<String>, String> {
    if std::env::var("THREADKEEPER_DEMO").is_ok_and(|v|v=="1"){return Ok(None);}
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(Some(token.trim().into()));
        }
    }
    read_credentials(&credentials_path()?)
}

fn connect(token: String) -> Result<String, String> {
    if token=="mock_token"{return Ok(token);}
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Could not start GitHub validation.".to_owned())?;
    runtime.block_on(async {
        let mut github = tracker_core::github::GitHub::new(token.clone())
            .map_err(|_| "This token contains invalid characters.".to_owned())?;
        github
            .validate_credentials()
            .await
            .map_err(|error| format!("Could not validate token: {error}"))?;
        Ok::<(), String>(())
    })?;
    save_credentials(&credentials_path()?, &token)?;
    if crate::data::authorized() {
        crate::data::replace_token(token.clone())?;
    }
    Ok(token)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Credentials {
    github_token: String,
}
fn credentials_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "Could not locate your home directory for token storage.".to_owned())?;
    let home=std::path::PathBuf::from(home);
    let current=home.join(".config/threadkeeper/credentials.json");
    let legacy=home.join(".config/powersync-workspace/credentials.json");
    Ok(if !current.exists() && legacy.exists() {legacy} else {current})
}
fn private_directory(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|_| "Could not create the private credentials directory.".to_owned())?;
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| "Could not inspect the credentials directory.".to_owned())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("The credentials directory must be a regular directory, not a link.".into());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not restrict access to the credentials directory.".into())
}
fn read_credentials(path: &std::path::Path) -> Result<Option<String>, String> {
    use std::{
        io::Read,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err("Could not read the saved token. Paste a token to replace it.".into());
        }
    };
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err("The credentials file must be a regular file.".into());
    }
    private_directory(path.parent().ok_or("Invalid credentials path")?)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "Could not restrict access to the credentials file.".to_owned())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(65536)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the saved token.".to_owned())?;
    let credentials: Credentials = serde_json::from_slice(&bytes)
        .map_err(|_| "The credentials file is invalid. Paste a token to replace it.".to_owned())?;
    let token = credentials.github_token.trim();
    Ok((!token.is_empty()).then(|| token.to_owned()))
}
fn save_credentials(path: &std::path::Path, token: &str) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let parent = path.parent().ok_or("Invalid credentials path")?;
    private_directory(parent)?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(".credentials-{}-{suffix}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "Could not create the private credentials file.".to_owned())?;
    let result = (|| -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&Credentials {
            github_token: token.into(),
        })?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err("Token is valid, but could not be saved to the credentials file. Check directory permissions and retry.".into());
    }
    Ok(())
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn private_credentials_roundtrip_replace_and_restrict_permissions() {
        let root =
            std::env::temp_dir().join(format!("workspace-credentials-test-{}", std::process::id()));
        let path = root.join("private/credentials.json");
        assert_eq!(read_credentials(&path).unwrap(), None);
        save_credentials(&path, "dummy-first").unwrap();
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            read_credentials(&path).unwrap().as_deref(),
            Some("dummy-first")
        );
        save_credentials(&path, "dummy-replacement").unwrap();
        assert_eq!(
            read_credentials(&path).unwrap().as_deref(),
            Some("dummy-replacement")
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        read_credentials(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::write(&path, "dummy-secret-invalid-json").unwrap();
        let error = read_credentials(&path).unwrap_err();
        assert!(!error.contains("dummy-secret"));
        let link = root.join("linked.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_credentials(&link).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

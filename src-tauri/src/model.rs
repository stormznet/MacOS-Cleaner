use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn operation_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
    pub modified: i64,
    pub nanos: i64,
}

impl Identity {
    pub fn from(m: &Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            modified: m.mtime(),
            nanos: m.mtime_nsec(),
        }
    }
    pub fn same_file(&self, other: &Self) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub root: PathBuf,
    pub category: String,
    pub bytes: u64,
    pub modified: u64,
    pub description: String,
    pub consequence: String,
    pub complete: bool,
    pub selectable: bool,
    pub issue: Option<String>,
    pub bundle_id: Option<String>,
    pub app_parent: Option<String>,
    pub version: Option<String>,
    pub identity: Identity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub operation: String,
    pub name: String,
    pub original: PathBuf,
    pub trash_path: Option<PathBuf>,
    pub identity: Identity,
    pub bytes: u64,
    pub timestamp: u64,
    pub status: String,
    pub error: Option<String>,
    pub bundle_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub roots: Vec<PathBuf>,
    pub appearance: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: vec![],
            appearance: "system".into(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Saved {
    pub settings: Settings,
    pub history: Vec<Record>,
}

pub struct Store {
    pub saved: Saved,
    pub items: HashMap<String, Item>,
}
pub struct AppState {
    pub home: PathBuf,
    pub data: PathBuf,
    pub store: Mutex<Store>,
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    _lock: fs::File,
}

impl AppState {
    pub fn new(home: PathBuf, data: PathBuf) -> Result<Self, String> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        clean_ancestry(&data)?;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(data.join("state.lock"))
            .map_err(|e| e.to_string())?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Mac Cleaner is already running. Use the existing window.".into());
        }
        let file = data.join("state.json");
        let saved = if file.exists() {
            serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Cannot read cleanup history; it has been preserved: {e}"))?
        } else {
            Saved::default()
        };
        Ok(Self {
            home,
            data,
            store: Mutex::new(Store {
                saved,
                items: HashMap::new(),
            }),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            _lock: lock,
        })
    }

    pub fn save(&self, saved: &Saved) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = self.data.join("state.json.tmp");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(saved).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(path, self.data.join("state.json")).map_err(|e| e.to_string())?;
        fs::File::open(&self.data)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }

    pub fn start(&self) -> Result<BusyGuard<'_>, String> {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "Another scan or file operation is still running")?;
        Ok(BusyGuard(&self.busy))
    }
}
pub struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub fn path_identity(path: &Path) -> Result<Identity, String> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if m.file_type().is_symlink() {
        return Err("Symbolic links are excluded".into());
    }
    Ok(Identity::from(&m))
}

pub fn clean_ancestry(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("Paths must be absolute and cannot contain parent traversal".into());
    }
    for p in path.ancestors() {
        path_identity(p)?;
    }
    Ok(())
}

pub fn is_node_modules_name(name: &str) -> bool {
    if name == "node_modules" {
        return true;
    }
    let Some(suffix) = name.strip_prefix("node_modules ") else {
        return false;
    };
    if !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let mut parts = suffix.split('-');
    for (width, max) in [(2, 23), (2, 59), (2, 59), (3, 999)] {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != width
            || !part.bytes().all(|c| c.is_ascii_digit())
            || !part.parse::<u16>().is_ok_and(|value| value <= max)
        {
            return false;
        }
    }
    parts.next().is_none()
}

pub fn validate_root(path: &Path, home: &Path) -> Result<PathBuf, String> {
    let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !root.is_dir()
        || root == home
        || !root.starts_with(home)
        || root.starts_with(home.join("Library"))
        || root.starts_with(home.join(".Trash"))
        || root.starts_with(home.join("Applications"))
        || root.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|name| matches!(name, ".git" | ".Trash") || is_node_modules_name(name))
        })
    {
        return Err("Choose a project folder inside your home folder, outside Library, Applications, and Trash".into());
    }
    clean_ancestry(&root)?;
    if path_identity(&root)?.dev != path_identity(home)?.dev {
        return Err("Project folders on other volumes are not supported yet".into());
    }
    Ok(root)
}

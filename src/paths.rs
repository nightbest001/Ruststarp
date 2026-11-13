//! Port of `Paths.cs`.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::app::PROJECT_NAME;

pub struct Paths {
    pub base: PathBuf,
    pub downloads: PathBuf,
    pub logs: PathBuf,
    pub versions: PathBuf,
    pub modifications: PathBuf,
    pub application: PathBuf,
}

static PATHS: OnceLock<Paths> = OnceLock::new();

pub fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn user_profile() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
}

pub fn process() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from(format!("{PROJECT_NAME}.exe")))
}

/// `%LOCALAPPDATA%\Ruststrap`
pub fn default_base() -> PathBuf {
    crate::registry::install_location().map(PathBuf::from).unwrap_or_else(|| local_app_data().join(PROJECT_NAME))
}

/// The folder Ruststrap is installed to when nothing else was chosen.
pub fn standard_base() -> PathBuf {
    local_app_data().join(PROJECT_NAME)
}

/// True when the running exe is the installed copy.
pub fn running_installed_copy() -> bool {
    let norm = |p: &std::path::Path| p.to_string_lossy().to_lowercase().replace('/', "\\");
    norm(&process()) == norm(&get().application)
}

pub fn init(base: PathBuf) {
    let p = Paths {
        downloads: base.join("Downloads"),
        logs: base.join("Logs"),
        versions: base.join("Versions"),
        modifications: base.join("Modifications"),
        application: base.join(format!("{PROJECT_NAME}.exe")),
        base,
    };
    let _ = PATHS.set(p);
}

pub fn get() -> &'static Paths {
    PATHS.get().expect("paths not initialised")
}

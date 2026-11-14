//! Port of `Bootstrapper.cs` and `AppData/*` (CLI only: progress is printed instead of drawn).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use md5::{Digest, Md5};
use regex::Regex;

use crate::app::{self, error_code};
use crate::deployment::{DeployError, Deployment, DEFAULT_CHANNEL};
use crate::fast_flags;
use crate::models::*;
use crate::paths;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const APP_SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<Settings>\r\n\t<ContentFolder>content</ContentFolder>\r\n\t<BaseUrl>http://www.roblox.com</BaseUrl>\r\n</Settings>\r\n";

// package name -> folder inside the version directory (found by hex-editing the stock bootstrapper)
const COMMON_MAP: &[(&str, &str)] = &[
    ("Libraries.zip", ""),
    ("redist.zip", ""),
    ("shaders.zip", "shaders\\"),
    ("ssl.zip", "ssl\\"),
    ("WebView2.zip", ""),
    ("WebView2RuntimeInstaller.zip", "WebView2RuntimeInstaller\\"),
    ("content-avatar.zip", "content\\avatar\\"),
    ("content-configs.zip", "content\\configs\\"),
    ("content-fonts.zip", "content\\fonts\\"),
    ("content-sky.zip", "content\\sky\\"),
    ("content-sounds.zip", "content\\sounds\\"),
    ("content-textures2.zip", "content\\textures\\"),
    ("content-models.zip", "content\\models\\"),
    ("content-textures3.zip", "PlatformContent\\pc\\textures\\"),
    ("content-terrain.zip", "PlatformContent\\pc\\terrain\\"),
    ("content-platform-fonts.zip", "PlatformContent\\pc\\fonts\\"),
    ("content-platform-dictionaries.zip", "PlatformContent\\pc\\shared_compression_dictionaries\\"),
    ("extracontent-luapackages.zip", "ExtraContent\\LuaPackages\\"),
    ("extracontent-translations.zip", "ExtraContent\\translations\\"),
    ("extracontent-models.zip", "ExtraContent\\models\\"),
    ("extracontent-textures.zip", "ExtraContent\\textures\\"),
    ("extracontent-places.zip", "ExtraContent\\places\\"),
];

const PLAYER_MAP: &[(&str, &str)] = &[("RobloxApp.zip", "")];

const STUDIO_MAP: &[(&str, &str)] = &[
    ("RobloxStudio.zip", ""),
    ("LibrariesQt5.zip", ""),
    ("content-studio_svg_textures.zip", "content\\studio_svg_textures\\"),
    ("content-qt_translations.zip", "content\\qt_translations\\"),
    ("content-api-docs.zip", "content\\api_docs\\"),
    ("extracontent-scripts.zip", "ExtraContent\\scripts\\"),
    ("studiocontent-models.zip", "StudioContent\\models\\"),
    ("studiocontent-textures.zip", "StudioContent\\textures\\"),
    ("BuiltInPlugins.zip", "BuiltInPlugins\\"),
    ("BuiltInStandalonePlugins.zip", "BuiltInStandalonePlugins\\"),
    ("ApplicationConfig.zip", "ApplicationConfig\\"),
    ("Plugins.zip", "Plugins\\"),
    ("Qml.zip", "Qml\\"),
    ("StudioFonts.zip", "StudioFonts\\"),
    ("RibbonConfig.zip", "RibbonConfig\\"),
];

// ------------------------------------------------------------------ app data

pub struct AppData {
    pub studio: bool,
    pub product_name: &'static str,
    pub binary_type: &'static str,
    pub registry_name: &'static str,
    pub process_name: &'static str,
    pub exe_name: &'static str,
    extra_map: &'static [(&'static str, &'static str)],
}

impl AppData {
    pub fn player() -> Self {
        Self {
            studio: false,
            product_name: "Roblox",
            binary_type: "WindowsPlayer",
            registry_name: "RobloxPlayer",
            process_name: "RobloxPlayerBeta",
            exe_name: "RobloxPlayerBeta.exe",
            extra_map: PLAYER_MAP,
        }
    }

    pub fn studio() -> Self {
        Self {
            studio: true,
            product_name: "Roblox Studio",
            binary_type: "WindowsStudio64",
            registry_name: "RobloxStudio",
            process_name: "RobloxStudioBeta",
            exe_name: "RobloxStudioBeta.exe",
            extra_map: STUDIO_MAP,
        }
    }

    pub fn package_dir(&self, package: &str) -> Option<&'static str> {
        self.extra_map.iter().chain(COMMON_MAP.iter()).find(|(k, _)| *k == package).map(|(_, v)| *v)
    }

    pub fn guid(&self) -> String {
        app::dist_state(self.studio).prop.version_guid.clone()
    }

    pub fn directory(&self) -> PathBuf {
        paths::get().versions.join(self.guid())
    }

    pub fn exe_path(&self) -> PathBuf {
        self.directory().join(self.exe_name)
    }

    pub fn mod_manifest(&self) -> Vec<String> {
        app::dist_state(self.studio).prop.mod_manifest.clone()
    }
}

// ------------------------------------------------------------------ helpers

fn norm(s: &str) -> String {
    s.replace('\\', "/").trim_start_matches('/').to_lowercase()
}

fn md5_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn list_logs(dir: &Path) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map_or(false, |x| x == "log") {
                set.insert(p);
            }
        }
    }
    set
}

// ------------------------------------------------------------------ bootstrapper

pub struct Bootstrapper {
    launch_mode: LaunchMode,
    launch_cmd: String,
    app: AppData,
    deployment: Deployment,
    latest_guid: String,
    latest_dir: PathBuf,
    manifest: Vec<Package>,
    no_connection: bool,
}

impl Bootstrapper {
    pub fn new(mode: LaunchMode) -> Self {
        let mut b = Self {
            launch_mode: mode,
            launch_cmd: app::launch().roblox_launch_args.clone(),
            app: AppData::player(),
            deployment: Deployment::new(),
            latest_guid: String::new(),
            latest_dir: PathBuf::new(),
            manifest: Vec::new(),
            no_connection: false,
        };
        b.setup_app_data();
        b
    }

    fn is_studio_launch(&self) -> bool {
        self.launch_mode != LaunchMode::Player
    }

    fn setup_app_data(&mut self) {
        self.app = if self.is_studio_launch() { AppData::studio() } else { AppData::player() };
        self.deployment.binary_type = self.app.binary_type.to_string();
    }

    /// Returns the process exit code.
    pub fn run(&mut self) -> i32 {
        match self.run_inner() {
            Ok(()) => error_code::SUCCESS,
            Err(e) => {
                crate::logger::write_exception("Bootstrapper::Run", &e);
                eprintln!("error: {e}");
                crate::progress::set_error(&e);
                error_code::INSTALL_FAILURE
            }
        }
    }

    fn run_inner(&mut self) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::Run";
        let launch = app::launch();

        if let Err(e) = self.deployment.init_connectivity() {
            crate::log!(IDENT, "Connectivity test failed: {e}");
            self.no_connection = true;
        }
        if !self.no_connection {
            self.fetch_current_channel();
            if let Err(e) = self.get_latest_version_info() {
                crate::log!(IDENT, "Could not get version info: {e}");
                self.no_connection = true;
            }
        }

        let guid = self.app.guid();
        let must_upgrade = launch.active("force")
            || app::state().prop.force_reinstall
            || guid.is_empty()
            || !self.app.exe_path().exists();

        if self.no_connection {
            if must_upgrade {
                return Err(crate::lang::pick(
                    "Cannot reach the Roblox servers and Roblox is not installed (or must be reinstalled). Check your connection and try again.",
                    "Nggak bisa nyambung ke server Roblox, dan Roblox belum terpasang (atau perlu dipasang ulang). Cek koneksimu lalu coba lagi.",
                ));
            }
            app::say(&crate::lang::pick(
                "No connection to Roblox servers, starting the installed version.",
                "Nggak bisa nyambung ke server Roblox, jalanin versi yang sudah terpasang.",
            ));
        } else if must_upgrade || guid != self.latest_guid {
            self.upgrade_roblox()?;
        }

        self.apply_modifications()?;

        if launch.active("nolaunch") {
            app::say(&crate::lang::pick("Done (-nolaunch).", "Selesai (-nolaunch)."));
            return Ok(());
        }
        self.start_roblox()
    }

    // -------------------------------------------------------------- channel / version

    fn channel_from_args(&self) -> Option<String> {
        if let Some(c) = app::launch().data("channel") {
            return Some(c.to_string());
        }
        let re = Regex::new(r"(?i)(?:^|[+&?,\s])channel:([^+&,\s]+)").ok()?;
        re.captures(&self.launch_cmd).map(|c| c[1].to_string())
    }

    fn fetch_current_channel(&mut self) {
        let ch = self.channel_from_args().filter(|c| !c.is_empty()).unwrap_or_else(|| DEFAULT_CHANNEL.to_string());
        crate::log!("Bootstrapper::FetchCurrentChannel", "Using channel {ch}");
        self.deployment.channel = ch;
    }

    fn get_latest_version_info(&mut self) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::GetLatestVersionInfo";
        let launch = app::launch();
        let new_guid: String;

        match launch.data("version") {
            Some(v) if launch.active("version") && !v.is_empty() => {
                crate::log!(IDENT, "Version set to {v} from arguments");
                new_guid = v.to_string();
            }
            _ => {
                let info = match self.deployment.get_info(None) {
                    Ok(i) => i,
                    Err(DeployError::InvalidChannel(code)) => {
                        crate::log!(IDENT, "Resetting channel from {} because {code}", self.deployment.channel);
                        self.deployment.channel = DEFAULT_CHANNEL.to_string();
                        self.deployment.get_info(None).map_err(|e| e.to_string())?
                    }
                    Err(e) => return Err(e.to_string()),
                };
                new_guid = info.version_guid;
            }
        }

        if new_guid != self.latest_guid {
            self.latest_guid = new_guid;
            self.latest_dir = paths::get().versions.join(&self.latest_guid);
            let url = self.deployment.get_location(&format!("/{}-rbxPkgManifest.txt", self.latest_guid));
            let text = app::http()
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
                .map_err(|e| e.to_string())?;
            self.manifest = parse_package_manifest(&text)?;
        }

        if self.launch_mode == LaunchMode::Unknown {
            if !self.manifest.is_empty() {
                let is_player = self.manifest.iter().any(|p| p.name == "RobloxApp.zip");
                crate::log!(IDENT, "Identified launch mode from package manifest: isPlayer = {is_player}");
                self.launch_mode = if is_player { LaunchMode::Player } else { LaunchMode::Studio };
                self.setup_app_data();
            } else {
                crate::log!(IDENT, "Could not identify launch mode as package manifest is empty");
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- install / upgrade

    fn kill_roblox_instances(&self) {
        let image = format!("{}.exe", self.app.process_name);
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/IM", image.as_str()]).creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.output();
    }

    pub fn cleanup_versions_folder() {
        let mut keep = HashSet::new();
        keep.insert(app::player_state().prop.version_guid.clone());
        keep.insert(app::studio_state().prop.version_guid.clone());
        let Ok(rd) = fs::read_dir(&paths::get().versions) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if keep.contains(&name) {
                continue;
            }
            crate::log!("Bootstrapper::CleanupVersionsFolder", "Removing old version {name}");
            let _ = fs::remove_dir_all(&p);
        }
    }

    fn upgrade_roblox(&mut self) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::UpgradeRoblox";
        let p = paths::get();
        for d in [&p.base, &p.downloads, &p.versions] {
            fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
        }
        app::say(&crate::lang::pick(
            &format!("Installing {} ({})...", self.app.product_name, self.latest_guid),
            &format!("Memasang {} ({})...", self.app.product_name, self.latest_guid),
        ));

        self.kill_roblox_instances();
        if self.latest_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&self.latest_dir) {
                crate::log!(IDENT, "Failed to delete the old version directory: {e}");
            }
        }
        fs::create_dir_all(&self.latest_dir).map_err(|e| e.to_string())?;

        let cached: Vec<String> = fs::read_dir(&p.downloads)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();

        let total = self.manifest.len();
        for (i, pkg) in self.manifest.iter().enumerate() {
            if crate::progress::cancelled() {
                return Err("Cancelled.".into());
            }
            crate::progress::set_stage(i, total);
            app::say(&crate::lang::pick(
                &format!("Downloading {} ({}/{})", pkg.name, i + 1, total),
                &format!("Mengunduh {} ({}/{})", pkg.name, i + 1, total),
            ));
            self.download_package(pkg)?;
            crate::progress::set_stage_progress(0.8);
            if pkg.name == "WebView2RuntimeInstaller.zip" {
                continue;
            }
            app::say(&crate::lang::pick(
                &format!("Installing {} ({}/{})", pkg.name, i + 1, total),
                &format!("Memasang {} ({}/{})", pkg.name, i + 1, total),
            ));
            self.extract_package(pkg, &self.latest_dir, None)?;
            crate::progress::set_stage_progress(1.0);
        }
        crate::progress::set_stage(total, total);

        fs::write(self.latest_dir.join("AppSettings.xml"), APP_SETTINGS).map_err(|e| e.to_string())?;

        if app::state().prop.prompt_web_view2_install && !crate::registry::webview2_installed() {
            app::say(&crate::lang::pick(
                "Note: Microsoft Edge WebView2 Runtime was not found. Install it from https://developer.microsoft.com/microsoft-edge/webview2/ if Roblox fails to show its menus.",
                "Catatan: Microsoft Edge WebView2 Runtime nggak ketemu. Kalau menu Roblox nggak muncul, pasang dari https://developer.microsoft.com/microsoft-edge/webview2/",
            ));
            let mut st = app::state();
            st.prop.prompt_web_view2_install = false;
        }

        {
            let mut ds = app::dist_state(self.app.studio);
            ds.prop.version_guid = self.latest_guid.clone();
            ds.prop.package_hashes.clear();
            for pkg in &self.manifest {
                ds.prop.package_hashes.insert(pkg.name.clone(), pkg.signature.clone());
            }
            ds.prop.size = self.manifest.iter().map(|x| (x.size + x.packed_size) / 1024).sum::<u64>() as i64;
        }

        Self::cleanup_versions_folder();

        if !app::settings().prop.debug_disable_version_package_cleanup {
            let mut all: HashSet<String> = HashSet::new();
            all.extend(app::player_state().prop.package_hashes.values().cloned());
            all.extend(app::studio_state().prop.package_hashes.values().cloned());
            for hash in &cached {
                if !all.contains(hash) {
                    let _ = fs::remove_file(p.downloads.join(hash));
                }
            }
        }

        let total_size = app::player_state().prop.size + app::studio_state().prop.size;
        crate::registry::set_estimated_size(total_size);

        {
            let mut st = app::state();
            st.prop.force_reinstall = false;
            st.save();
        }
        app::dist_state(self.app.studio).save();
        Ok(())
    }

    fn download_package(&self, pkg: &Package) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::DownloadPackage";
        let dest = pkg.download_path();

        if dest.exists() {
            if md5_file(&dest).map(|h| h == pkg.signature).unwrap_or(false) {
                crate::log!(IDENT, "{} already downloaded", pkg.name);
                return Ok(());
            }
            let _ = fs::remove_file(&dest);
        } else {
            let roblox_copy = paths::local_app_data().join("Roblox").join("Downloads").join(&pkg.signature);
            if roblox_copy.exists() {
                crate::log!(IDENT, "Copying {} from the official Roblox download cache", pkg.name);
                fs::copy(&roblox_copy, &dest).map_err(|e| e.to_string())?;
                return Ok(());
            }
        }

        let url = self.deployment.get_location(&format!("/{}-{}", self.latest_guid, pkg.name));
        const MAX_TRIES: u32 = 5;
        let mut last_err = String::new();
        for attempt in 1..=MAX_TRIES {
            match Self::try_download(&url, &dest, pkg) {
                Ok(()) => return Ok(()),
                Err((e, fatal)) => {
                    let _ = fs::remove_file(&dest);
                    crate::log!(IDENT, "Attempt {attempt}/{MAX_TRIES} for {} failed: {e}", pkg.name);
                    if fatal {
                        return Err(e);
                    }
                    last_err = e;
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
        Err(last_err)
    }

    /// Err((message, fatal)); a checksum mismatch is fatal, network errors are retried.
    fn try_download(url: &str, dest: &Path, pkg: &Package) -> Result<(), (String, bool)> {
        let mut resp = app::http().get(url).send().map_err(|e| (e.to_string(), false))?;
        if !resp.status().is_success() {
            return Err((format!("HTTP {} for {url}", resp.status()), false));
        }
        let mut file = File::create(dest).map_err(|e| (e.to_string(), false))?;
        let mut hasher = Md5::new();
        let mut buf = vec![0u8; 65536];
        let expected = resp.content_length().filter(|n| *n > 0).unwrap_or(pkg.packed_size.max(1));
        let mut got: u64 = 0;
        loop {
            if crate::progress::cancelled() {
                return Err(("Cancelled.".to_string(), true));
            }
            let n = resp.read(&mut buf).map_err(|e| (e.to_string(), false))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| (e.to_string(), false))?;
            hasher.update(&buf[..n]);
            got += n as u64;
            crate::progress::set_stage_progress(0.8 * (got as f32 / expected as f32).min(1.0));
        }
        let hash = format!("{:x}", hasher.finalize());
        if hash != pkg.signature {
            return Err((
                format!("Failed to verify download of {url}\nExpected hash: {}\nGot hash: {hash}", pkg.signature),
                true,
            ));
        }
        Ok(())
    }

    /// Extracts a package into `root`. With `files`, only those (normalised, root-relative) paths are written.
    fn extract_package(&self, pkg: &Package, root: &Path, files: Option<&HashSet<String>>) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::ExtractPackage";
        let Some(dir) = self.app.package_dir(&pkg.name) else {
            crate::log!(IDENT, "No mapping for {}, skipping", pkg.name);
            return Ok(());
        };
        let target = root.join(dir.replace('\\', "/"));
        let f = File::open(pkg.download_path()).map_err(|e| format!("{}: {e}", pkg.name))?;
        let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("{}: {e}", pkg.name))?;

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = entry.enclosed_name() else { continue };
            if entry.is_dir() {
                if files.is_none() {
                    fs::create_dir_all(target.join(&rel)).map_err(|e| e.to_string())?;
                }
                continue;
            }
            let rel_str = rel.to_string_lossy().to_string();
            if let Some(set) = files {
                if !set.contains(&norm(&format!("{dir}{rel_str}"))) {
                    continue;
                }
            }
            let out = target.join(&rel);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let res = File::create(&out).and_then(|mut o| std::io::copy(&mut entry, &mut o).map(|_| ()));
            if let Err(e) = res {
                // only fonts get a pass, same as the original
                if rel_str.to_lowercase().ends_with(".ttf") {
                    crate::log!(IDENT, "Failed to extract {rel_str}: {e}");
                } else {
                    return Err(format!("failed to extract {rel_str} from {}: {e}", pkg.name));
                }
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- modifications

    fn apply_modifications(&mut self) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::ApplyModifications";
        let mods = paths::get().modifications.clone();
        fs::create_dir_all(&mods).map_err(|e| e.to_string())?;

        let use_flags = app::settings().prop.use_fast_flag_manager;
        {
            let mut ff = app::fast_flags();
            if use_flags && !ff.prop.is_empty() {
                fast_flags::normalise(&mut ff.prop);
                ff.save();
            } else {
                let _ = fs::remove_file(&ff.file_location);
            }
        }

        let version_dir = self.app.directory();
        if !version_dir.exists() {
            return Ok(());
        }

        {
            let guid = app::dist_state(self.app.studio).prop.version_guid.clone();
            crate::fonts::prepare_mods(&version_dir, &guid, self.app.studio);
        }

        let mut current = Vec::new();
        walk(&mods, &mods, &mut current);
        let current_set: HashSet<String> = current.iter().map(|s| norm(s)).collect();

        // files that were applied before but are gone from Modifications: restore the originals
        let removed: Vec<String> = self.app.mod_manifest().into_iter().filter(|p| !current_set.contains(&norm(p))).collect();
        if !removed.is_empty() {
            crate::log!(IDENT, "{} modified file(s) were removed, restoring originals", removed.len());
            for r in &removed {
                let _ = fs::remove_file(version_dir.join(r));
            }
            if self.manifest.is_empty() {
                crate::log!(IDENT, "No package manifest available, cannot restore original files now");
            } else {
                let set: HashSet<String> = removed.iter().map(|s| norm(s)).collect();
                for pkg in &self.manifest {
                    if pkg.download_path().exists() && pkg.name != "WebView2RuntimeInstaller.zip" {
                        self.extract_package(pkg, &version_dir, Some(&set))?;
                    }
                }
            }
        }

        for rel in &current {
            let src = mods.join(rel);
            let dst = version_dir.join(rel);
            let needs = match (fs::metadata(&src), fs::metadata(&dst)) {
                (Ok(s), Ok(d)) => s.len() != d.len() || s.modified().ok() != d.modified().ok(),
                (Ok(_), Err(_)) => true,
                _ => false,
            };
            if needs {
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::copy(&src, &dst) {
                    crate::log!(IDENT, "Failed to apply {rel}: {e}");
                }
            }
        }

        let mut ds = app::dist_state(self.app.studio);
        ds.prop.mod_manifest = current;
        ds.save();
        Ok(())
    }

    // -------------------------------------------------------------- launch

    fn start_roblox(&mut self) -> Result<(), String> {
        const IDENT: &str = "Bootstrapper::StartRoblox";
        crate::progress::set_can_cancel(false);
        crate::progress::set_indeterminate();
        let exe = self.app.exe_path();
        crate::gpu::apply_launch(&exe);
        let mut cmd = Command::new(&exe);
        cmd.current_dir(self.app.directory());
        if !self.launch_cmd.is_empty() {
            cmd.raw_arg(&self.launch_cmd);
        }

        if self.launch_mode == LaunchMode::StudioAuth {
            cmd.spawn().map_err(|e| e.to_string())?;
            return Ok(());
        }

        // the tray process stays after this window closes, and holds the Roblox mutex for Multi Account
        if !self.is_studio_launch() {
            crate::tray::ensure_running();
        }

        let rbx_logs = paths::local_app_data().join("Roblox").join("logs");
        let _ = fs::create_dir_all(&rbx_logs);
        let before = list_logs(&rbx_logs);

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                // force a reinstall next time, the executable may be damaged
                let _ = fs::remove_file(&exe);
                return Err(format!("failed to start {}: {e}", self.app.product_name));
            }
        };
        let pid = child.id();
        crate::log!(IDENT, "Started Roblox (PID {pid}), waiting for log file");
        if !self.is_studio_launch() {
            crate::boost::apply(pid);
        }
        app::say(&crate::lang::pick(
            &format!("Launched {}.", self.app.product_name),
            &format!("{} sudah jalan.", self.app.product_name),
        ));

        let deadline = Instant::now() + Duration::from_secs(15);
        let mut log_file: Option<PathBuf> = None;
        while Instant::now() < deadline {
            if let Some(p) = list_logs(&rbx_logs).into_iter().find(|p| !before.contains(p)) {
                log_file = Some(p);
                break;
            }
            thread::sleep(Duration::from_millis(250));
        }
        let Some(log_file) = log_file else {
            crate::log!(IDENT, "Unable to identify log file");
            return Ok(());
        };
        crate::log!(IDENT, "Got log file as {}", log_file.display());

        if self.is_studio_launch() {
            return Ok(());
        }

        let (integrations, tracking) = {
            let s = app::settings();
            (s.prop.custom_integrations.clone(), s.prop.enable_activity_tracking)
        };

        let mut autoclose = Vec::new();
        for it in &integrations {
            crate::log!(IDENT, "Launching custom integration '{}' ({} {})", it.name, it.location, it.launch_args);
            let mut c = Command::new(&it.location);
            c.raw_arg(it.launch_args.replace("\r\n", " "));
            if let Some(dir) = Path::new(&it.location).parent() {
                c.current_dir(dir);
            }
            match c.spawn() {
                Ok(ch) => {
                    if it.auto_close {
                        autoclose.push(ch.id());
                    }
                }
                Err(e) => crate::log!(IDENT, "Failed to launch integration '{}': {e}", it.name),
            }
        }

        let testmode = app::launch().active("testmode");
        if tracking || testmode || !autoclose.is_empty() {
            let data = WatcherData { process_id: pid, log_file: Some(log_file.to_string_lossy().to_string()), autoclose_pids: autoclose };
            let b64 = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&data).unwrap_or_default());
            let mut w = Command::new(paths::process());
            w.arg("-watcher").arg(&b64).creation_flags(CREATE_NO_WINDOW);
            if testmode {
                w.arg("-testmode");
            }
            if let Err(e) = w.spawn() {
                crate::log!(IDENT, "Failed to start the watcher: {e}");
            }
        }
        // performance overlay (CPU / GPU / RAM), its own small process
        let show_overlay = {
            let s = app::settings();
            s.prop.show_cpu || s.prop.show_gpu || s.prop.show_ram || s.prop.show_fps
        };
        if show_overlay {
            let mut o = Command::new(paths::process());
            o.arg("-overlay").creation_flags(CREATE_NO_WINDOW);
            if let Err(e) = o.spawn() {
                crate::log!(IDENT, "Failed to start the overlay: {e}");
            }
        }
        thread::sleep(Duration::from_secs(1));
        Ok(())
    }
}

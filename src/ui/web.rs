//! Web front end. Every window (settings, launch menu, bootstrapper progress, setup wizard) is the same
//! HTML/CSS/JS page (`ui-web/dist/index.html`) shown in a WebView2 window, so animation and layout are
//! the same as Bloxstrap's WPF-UI. Rust only does the work: the page asks through `window.ipc` and
//! gets the answer back through `window.__reply`.
//!
//! Edit the page in `ui-web/` and run `python3 ui-web/build.py` to rebuild `ui-web/dist/index.html`.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use std::thread;

use serde_json::{json, Map, Value};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::WindowBuilderExtWindows;
use tao::window::{Icon, ResizeDirection, WindowBuilder};
use wry::{NewWindowResponse, WebContext, WebViewBuilder, WebViewBuilderExtWindows};

use crate::app::{self, error_code, PROJECT_DOWNLOAD_LINK, PROJECT_HELP_LINK, PROJECT_NAME, PROJECT_SUPPORT_LINK};
use crate::bootstrapper::Bootstrapper;
use crate::models::{CustomIntegration, FastFlags, LaunchMode, Settings};
use crate::lang::pick;
use crate::{accounts, fast_flags, gpu, installer, paths, progress, registry, shortcuts, updater};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const HTML: &str = include_str!("../../ui-web/dist/index.html");
const ICON_PNG: &[u8] = include_bytes!("../../assets/Ruststrap-256.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Settings,
    Home,
    Launch(LaunchMode),
    Setup,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Settings => "settings",
            Mode::Home => "home",
            Mode::Launch(_) => "launch",
            Mode::Setup => "setup",
        }
    }
    /// Closing the window (title bar X, Alt+F4) asks the page first, because it may have unsaved work.
    fn asks_before_close(self) -> bool {
        matches!(self, Mode::Settings | Mode::Setup)
    }
}

enum UserEvent {
    Eval(String),
    Show,
    Min,
    Max,
    Drag,
    Resize(ResizeDirection),
    Close,
    /// Window X pressed without the page's say-so.
    CloseRequest,
    /// The update banner appeared (true) or went away (false): the launch menu grows to make room.
    Banner(bool),
}

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
static INSTALLED_EXE: Mutex<Option<PathBuf>> = Mutex::new(None);
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ------------------------------------------------------------------ icon / theme helpers

pub(crate) fn window_icon() -> Option<Icon> {
    let mut dec = png::Decoder::new(ICON_PNG);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        _ => return None,
    };
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

fn system_is_dark() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 0)
        .unwrap_or(true)
}

fn is_dark(theme: &str) -> bool {
    match theme {
        "dark" => true,
        "light" => false,
        _ => system_is_dark(),
    }
}

// ------------------------------------------------------------------ settings <-> JSON (snake_case for the page)

fn settings_to_json(s: &Settings) -> Value {
    let ints: Vec<Value> = s
        .custom_integrations
        .iter()
        .map(|c| json!({"name": c.name, "location": c.location, "launch_args": c.launch_args, "auto_close": c.auto_close}))
        .collect();
    json!({
        "check_for_updates": s.check_for_updates,
        "confirm_launches": s.confirm_launches,
        "use_fast_flag_manager": s.use_fast_flag_manager,
        "enable_activity_tracking": s.enable_activity_tracking,
        "use_disable_app_patch": s.use_disable_app_patch,
        "use_discord_rich_presence": s.use_discord_rich_presence,
        "allow_activity_joining": s.allow_activity_joining,
        "show_account_on_rich_presence": s.show_account_on_rich_presence,
        "keep_in_tray": s.keep_in_tray,
        "multi_instance": s.multi_instance,
        "show_cpu": s.show_cpu,
        "show_gpu": s.show_gpu,
        "show_ram": s.show_ram,
        "gpu_preference": s.gpu_preference,
        "theme": s.theme,
        "language": s.language,
        "bootstrapper_title": s.bootstrapper_title,
        "debug_disable_version_package_cleanup": s.debug_disable_version_package_cleanup,
        "custom_integrations": ints,
    })
}

fn apply_settings_json(s: &mut Settings, v: &Value) {
    let b = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
    let st = |k: &str, d: &str| v.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
    s.check_for_updates = b("check_for_updates", s.check_for_updates);
    s.confirm_launches = b("confirm_launches", s.confirm_launches);
    s.use_fast_flag_manager = b("use_fast_flag_manager", s.use_fast_flag_manager);
    s.enable_activity_tracking = b("enable_activity_tracking", s.enable_activity_tracking);
    s.use_disable_app_patch = b("use_disable_app_patch", s.use_disable_app_patch);
    s.use_discord_rich_presence = b("use_discord_rich_presence", s.use_discord_rich_presence);
    s.allow_activity_joining = b("allow_activity_joining", s.allow_activity_joining);
    s.show_account_on_rich_presence = b("show_account_on_rich_presence", s.show_account_on_rich_presence);
    s.keep_in_tray = b("keep_in_tray", s.keep_in_tray);
    s.multi_instance = b("multi_instance", s.multi_instance);
    s.show_cpu = b("show_cpu", s.show_cpu);
    s.show_gpu = b("show_gpu", s.show_gpu);
    s.show_ram = b("show_ram", s.show_ram);
    s.debug_disable_version_package_cleanup =
        b("debug_disable_version_package_cleanup", s.debug_disable_version_package_cleanup);
    s.gpu_preference = st("gpu_preference", &s.gpu_preference.clone());
    s.theme = st("theme", &s.theme.clone());
    s.language = st("language", &s.language.clone());
    if !matches!(s.language.as_str(), "system" | "en" | "id") {
        s.language = "system".to_string();
    }
    s.bootstrapper_title = st("bootstrapper_title", &s.bootstrapper_title.clone());
    if let Some(arr) = v.get("custom_integrations").and_then(Value::as_array) {
        s.custom_integrations = arr
            .iter()
            .map(|c| CustomIntegration {
                name: c.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                location: c.get("location").and_then(Value::as_str).unwrap_or("").to_string(),
                launch_args: c.get("launch_args").and_then(Value::as_str).unwrap_or("").to_string(),
                auto_close: c.get("auto_close").and_then(Value::as_bool).unwrap_or(true),
            })
            .collect();
    }
}

fn flags_to_json(f: &FastFlags) -> Value {
    let mut m = Map::new();
    for (k, v) in f {
        let s = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        m.insert(k.clone(), Value::String(s));
    }
    Value::Object(m)
}

// ------------------------------------------------------------------ init data

fn init_json(mode: Mode) -> Value {
    let settings = app::settings().prop.clone();
    let installed = paths::get().application.exists();
    let (confirm, title) = {
        let t = settings.bootstrapper_title.trim().to_string();
        let ask = settings.confirm_launches && !app::launch_opt().map(|l| l.roblox_launch_args.is_empty()).unwrap_or(true);
        (ask && matches!(mode, Mode::Launch(LaunchMode::Player)), if t.is_empty() { PROJECT_NAME.to_string() } else { t })
    };
    let adapters: Vec<Value> = if mode == Mode::Settings {
        gpu::detect()
            .into_iter()
            .map(|a| {
                let kind = match a.kind {
                    gpu::Kind::Discrete => "discrete",
                    gpu::Kind::Integrated => "integrated",
                    gpu::Kind::Other => "other",
                };
                json!({"name": a.name, "vendor": a.vendor, "kind": kind, "vram_mb": a.vram_mb})
            })
            .collect()
    } else {
        Vec::new()
    };
    let sc = if mode == Mode::Settings { shortcuts::current() } else { [false; 5] };
    let flags = if mode == Mode::Settings { flags_to_json(&app::fast_flags().prop) } else { json!({}) };
    let font = if mode == Mode::Settings { crate::fonts::state_json() } else { json!({}) };
    let accounts = if mode == Mode::Settings { accounts::list_json() } else { json!([]) };
    // `ruststrap -menu accounts` (the tray menu) opens the settings window on that page
    let start_page = app::launch_opt().and_then(|l| l.data("menu")).unwrap_or("").to_string();
    let account_last = json!({"place": settings.account_place, "job": settings.account_job});
    json!({
        "mode": mode.name(),
        "name": PROJECT_NAME,
        "version": app::VERSION,
        "installed": installed,
        "can_update": matches!(mode, Mode::Home | Mode::Settings) && paths::running_installed_copy(),
        "system_lang": if crate::lang::system_is_indonesian() { "id" } else { "en" },
        "already_running": false,
        "settings": settings_to_json(&settings),
        "force_reinstall": app::state().prop.force_reinstall,
        "flags": flags,
        "font": font,
        "accounts": accounts,
        "start_page": start_page,
        "account_last": account_last,
        "shortcuts": sc,
        "adapters": adapters,
        "paths": {
            "base": paths::get().base.to_string_lossy(),
            "logs": paths::get().logs.to_string_lossy(),
            "mods": paths::get().modifications.to_string_lossy(),
        },
        "links": {"wiki": PROJECT_HELP_LINK, "repo": PROJECT_DOWNLOAD_LINK, "issues": PROJECT_SUPPORT_LINK},
        "default_location": paths::default_base().to_string_lossy(),
        "confirm": confirm,
        "title": title,
    })
}

// ------------------------------------------------------------------ commands

fn open_path(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let _ = Command::new("explorer").arg(path).spawn();
}

fn open_url(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(url).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

fn arg_str<'a>(a: &'a Value, k: &str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or("")
}
fn arg_u64(a: &Value, k: &str) -> u64 {
    a.get(k).and_then(Value::as_u64).unwrap_or(0)
}
fn arg_bool(a: &Value, k: &str) -> bool {
    a.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn run_install(dir: String, desktop: bool, start_menu: bool) -> Result<(), String> {
    let dir = dir.trim().to_string();
    std::fs::create_dir_all(&dir).map_err(|e| pick(&format!("Cannot create {dir}: {e}"), &format!("Gagal bikin folder {dir}: {e}")))?;
    registry::set_install_location(&dir).map_err(|e| pick(&format!("Cannot write the registry: {e}"), &format!("Gagal nulis ke registry: {e}")))?;
    let mut c = Command::new(paths::process());
    c.arg("install").arg("-quiet");
    if desktop {
        c.arg("-desktop");
    }
    if start_menu {
        c.arg("-startmenu");
    }
    let out = c.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| pick(&format!("Cannot start the installer: {e}"), &format!("Gagal menjalankan installer: {e}")))?;
    match out.status.code() {
        Some(code) if code == error_code::SUCCESS => {
            *INSTALLED_EXE.lock().unwrap() = Some(PathBuf::from(&dir).join(format!("{PROJECT_NAME}.exe")));
            Ok(())
        }
        _ => Err(pick(
            &format!(
                "{PROJECT_NAME} has been installed to this location before and is still present, however the installer \
                 cannot overwrite the old executable.\n\nPlease manually delete {PROJECT_NAME}.exe from the install \
                 location or try restarting your system, and then retry installation afterwards."
            ),
            &format!(
                "{PROJECT_NAME} pernah dipasang di folder ini dan filenya masih ada, tapi installer nggak bisa menimpa \
                 file lama itu.\n\nHapus {PROJECT_NAME}.exe dari folder tersebut secara manual atau restart \
                 komputermu, lalu coba pasang lagi."
            ),
        )),
    }
}

fn save(a: &Value) -> Result<Value, String> {
    let mut settings = app::settings().prop.clone();
    apply_settings_json(&mut settings, a.get("settings").unwrap_or(&Value::Null));
    let pref = gpu::Pref::parse(&settings.gpu_preference).unwrap_or(gpu::Pref::Default);
    settings.gpu_preference = pref.as_str().to_string();

    let mut flags: FastFlags = FastFlags::new();
    if let Some(o) = a.get("flags").and_then(Value::as_object) {
        for (k, v) in o {
            flags.insert(k.clone(), Value::String(v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())));
        }
    }
    fast_flags::normalise(&mut flags);

    {
        let mut s = app::settings();
        s.prop = settings;
        s.save();
    }
    {
        let mut st = app::state();
        st.prop.force_reinstall = arg_bool(a, "force_reinstall");
        st.save();
    }
    {
        let mut ff = app::fast_flags();
        ff.prop = flags;
        ff.save();
    }
    crate::lang::init(&app::settings().prop.language);
    let mut want = [false; 5];
    if let Some(arr) = a.get("shortcuts").and_then(Value::as_array) {
        for (i, v) in arr.iter().take(5).enumerate() {
            want[i] = v.as_bool().unwrap_or(false);
        }
    }
    let mut failed = gpu::apply_installed(pref);
    failed.extend(shortcuts::apply(&want));
    Ok(json!({"failed": failed}))
}

fn snapshot_json() -> Value {
    let s = progress::snapshot();
    json!({
        "message": s.message,
        "fraction": s.fraction,
        "can_cancel": s.can_cancel,
        "error": s.error,
        "finished": s.finished,
    })
}

fn handle(mode: Mode, proxy: &EventLoopProxy<UserEvent>, cmd: &str, a: &Value) -> Result<Value, String> {
    match cmd {
        "ready" => {
            let _ = proxy.send_event(UserEvent::Show);
            Ok(Value::Null)
        }
        "save" => save(a),
        "overlay_preview" => {
            Command::new(paths::process())
                .args(["-overlay", "preview"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "open_url" => {
            open_url(arg_str(a, "url"));
            Ok(Value::Null)
        }
        "open_folder" => {
            let p = paths::get();
            match arg_str(a, "which") {
                "logs" => open_path(&p.logs),
                "mods" => open_path(&p.modifications),
                _ => open_path(&p.base),
            }
            Ok(Value::Null)
        }
        "font_choose" => {
            let Some(p) = rfd::FileDialog::new().add_filter("Font", &["ttf", "otf"]).pick_file() else { return Ok(Value::Null) };
            crate::fonts::set_custom_font(&p)?;
            Ok(crate::fonts::state_json())
        }
        "font_remove" => {
            crate::fonts::reset_custom_font()?;
            Ok(crate::fonts::state_json())
        }
        "browse_file" => Ok(rfd::FileDialog::new().pick_file().map(|p| json!(p.to_string_lossy())).unwrap_or(Value::Null)),
        "browse_folder" => Ok(rfd::FileDialog::new().pick_folder().map(|p| json!(p.to_string_lossy())).unwrap_or(Value::Null)),
        "read_file" => {
            let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json", "txt"]).pick_file() else { return Ok(Value::Null) };
            std::fs::read_to_string(p).map(Value::String).map_err(|e| e.to_string())
        }
        "export_diag" => {
            let Some(dest) = rfd::FileDialog::new()
                .set_file_name("Ruststrap-diagnostics.zip")
                .add_filter("Zip", &["zip"])
                .save_file()
            else {
                return Ok(Value::Null);
            };
            crate::diagnostics::export(&dest, arg_bool(a, "config"), arg_bool(a, "logs"))
                .map(|_| json!(true))
                .map_err(|e| e.to_string())
        }
        "install" => {
            if installer::install(true) == error_code::SUCCESS {
                Ok(Value::Null)
            } else {
                Err(pick("Install failed, see the log", "Pemasangan gagal, cek lognya"))
            }
        }
        "uninstall" => {
            if installer::uninstall(true, arg_bool(a, "keep_data")) == error_code::SUCCESS {
                Ok(Value::Null)
            } else {
                Err(pick("Uninstall failed, see the log", "Uninstall gagal, cek lognya"))
            }
        }
        "check_update" => match updater::check() {
            Ok(Some(i)) => Ok(json!({"available": true, "version": i.version, "current": app::VERSION, "size": i.size})),
            Ok(None) => Ok(json!({"available": false, "current": app::VERSION})),
            Err(e) => Err(e),
        },
        "start_update" => {
            // the same window comes back after the swap: settings reopens as settings, the menu as the menu
            let reopen = if mode == Mode::Settings { Some("-menu") } else { None };
            let p = proxy.clone();
            updater::start(move |ok| {
                if ok {
                    let mut c = Command::new(&paths::get().application);
                    if let Some(arg) = reopen {
                        c.arg(arg);
                    }
                    let _ = c.spawn();
                    let _ = p.send_event(UserEvent::Close);
                }
            })?;
            Ok(Value::Null)
        }
        "update_progress" => {
            let p = updater::progress();
            Ok(json!({"phase": updater::phase_name(p.phase), "done": p.done, "total": p.total, "error": p.error}))
        }
        "accounts_list" => Ok(accounts::list_json()),
        "account_add_cookie" => accounts::add_cookie(arg_str(a, "cookie")),
        "account_add_password" => accounts::add_password(arg_str(a, "username"), arg_str(a, "password")),
        "account_add_window" => accounts::add_window(),
        "account_remove" => {
            accounts::remove(arg_u64(a, "user_id"));
            Ok(Value::Null)
        }
        "account_check" => accounts::check(arg_u64(a, "user_id")).map(Value::Bool),
        "account_launch" => {
            accounts::launch(arg_u64(a, "user_id"), arg_str(a, "place"), arg_str(a, "job"))?;
            Ok(Value::Null)
        }
        "launch" => {
            let arg = arg_str(a, "arg");
            if matches!(arg, "-player" | "-studio" | "-settings") {
                let _ = Command::new(paths::process()).arg(arg).spawn();
                let _ = proxy.send_event(UserEvent::Close);
            }
            Ok(Value::Null)
        }
        "progress" => Ok(snapshot_json()),
        "start" => {
            if let Mode::Launch(lm) = mode {
                if !STARTED.swap(true, Ordering::SeqCst) {
                    thread::spawn(move || {
                        let code = Bootstrapper::new(lm).run();
                        progress::finish(code);
                    });
                }
            }
            Ok(Value::Null)
        }
        "cancel" => {
            progress::cancel();
            if !STARTED.load(Ordering::SeqCst) {
                progress::finish(error_code::CANCELLED);
                let _ = proxy.send_event(UserEvent::Close);
            }
            Ok(Value::Null)
        }
        "setup_install" => {
            run_install(arg_str(a, "location").to_string(), arg_bool(a, "desktop"), arg_bool(a, "start"))?;
            Ok(Value::Null)
        }
        "setup_finish" => {
            let exe = INSTALLED_EXE.lock().unwrap().clone();
            if let Some(exe) = exe {
                let arg = if a.get("after").and_then(Value::as_i64) == Some(0) { "-player" } else { "-menu" };
                let _ = Command::new(exe).arg(arg).spawn();
            }
            let _ = proxy.send_event(UserEvent::Close);
            Ok(Value::Null)
        }
        "win" => {
            let ev = match arg_str(a, "op") {
                "min" => UserEvent::Min,
                "max" => UserEvent::Max,
                "drag" => UserEvent::Drag,
                "banner" => UserEvent::Banner(arg_bool(a, "on")),
                "close" if arg_bool(a, "force") || !mode.asks_before_close() => UserEvent::Close,
                "close" => UserEvent::CloseRequest,
                "resize" => UserEvent::Resize(match arg_str(a, "dir") {
                    "n" => ResizeDirection::North,
                    "s" => ResizeDirection::South,
                    "e" => ResizeDirection::East,
                    "w" => ResizeDirection::West,
                    "nw" => ResizeDirection::NorthWest,
                    "ne" => ResizeDirection::NorthEast,
                    "sw" => ResizeDirection::SouthWest,
                    _ => ResizeDirection::SouthEast,
                }),
                _ => return Ok(Value::Null),
            };
            let _ = proxy.send_event(ev);
            Ok(Value::Null)
        }
        other => Err(format!("unknown command: {other}")),
    }
}

// ------------------------------------------------------------------ window

/// (width, height, min width, min height, resizable)
fn geometry(mode: Mode) -> (f64, f64, f64, f64, bool) {
    match mode {
        Mode::Settings => (1000.0, 580.0, 900.0, 480.0, true),
        Mode::Home => (580.0, 340.0, 580.0, 340.0, false),
        Mode::Launch(_) => (500.0, 280.0, 500.0, 280.0, false),
        Mode::Setup => (840.0, 540.0, 840.0, 460.0, true),
    }
}

/// Opens the window for `mode`, runs until it closes and returns the process exit code.
pub fn run(mode: Mode) -> i32 {
    let settings = app::settings().prop.clone();
    let dark = is_dark(&settings.theme);
    let (w, h, minw, minh, resizable) = geometry(mode);
    let title = match mode {
        Mode::Launch(_) => {
            let t = settings.bootstrapper_title.trim();
            if t.is_empty() { PROJECT_NAME.to_string() } else { t.to_string() }
        }
        Mode::Settings => format!("{PROJECT_NAME} Settings"),
        Mode::Setup => format!("{PROJECT_NAME} Installer"),
        Mode::Home => PROJECT_NAME.to_string(),
    };

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let mut wb = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(LogicalSize::new(w, h))
        .with_resizable(resizable)
        .with_maximizable(resizable)
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_visible(false)
        .with_window_icon(window_icon());
    if resizable {
        wb = wb.with_min_inner_size(LogicalSize::new(minw, minh));
    }
    let window = match wb.build(&event_loop) {
        Ok(w) => w,
        Err(e) => {
            crate::log!("Ui::Web", "Window failed: {e}");
            return fallback(mode);
        }
    };
    center(&window);

    let data_dir = paths::get().base.join("WebView2");
    let mut ctx = WebContext::new(Some(data_dir));
    let init = format!("window.__INIT={};", init_json(mode));
    let ipc_proxy = proxy.clone();
    let bg = if dark { (32, 32, 32, 255) } else { (243, 243, 243, 255) };
    let webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_background_color(bg)
        .with_initialization_script(init)
        .with_html(HTML)
        .with_browser_accelerator_keys(false)
        .with_new_window_req_handler(|url, _| {
            open_url(&url);
            NewWindowResponse::Deny
        })
        .with_ipc_handler(move |req| {
            let Ok(msg) = serde_json::from_str::<Value>(req.body()) else { return };
            let id = msg.get("id").and_then(Value::as_i64).unwrap_or(0);
            let cmd = msg.get("cmd").and_then(Value::as_str).unwrap_or("").to_string();
            let args = msg.get("args").cloned().unwrap_or(Value::Null);
            let p = ipc_proxy.clone();
            // run off the UI thread: installs, file dialogs and registry work can take a while
            thread::spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(mode, &p, &cmd, &args)))
                    .unwrap_or_else(|_| Err("internal error".to_string()));
                let js = match res {
                    Ok(v) => format!("window.__reply({id},true,{v})"),
                    Err(e) => format!("window.__reply({id},false,{})", Value::String(e)),
                };
                let _ = p.send_event(UserEvent::Eval(js));
            });
        })
        .build(&window);
    let webview = match webview {
        Ok(v) => v,
        Err(e) => {
            crate::log!("Ui::Web", "WebView2 failed: {e}");
            drop(window);
            return fallback(mode);
        }
    };

    event_loop.run_return(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(ev) => match ev {
                UserEvent::Eval(js) => {
                    let _ = webview.evaluate_script(&js);
                }
                UserEvent::Show => {
                    window.set_visible(true);
                    window.set_focus();
                }
                UserEvent::Min => window.set_minimized(true),
                UserEvent::Max => window.set_maximized(!window.is_maximized()),
                UserEvent::Drag => {
                    let _ = window.drag_window();
                }
                UserEvent::Resize(d) => {
                    let _ = window.drag_resize_window(d);
                }
                UserEvent::Close => {
                    *flow = ControlFlow::Exit;
                }
                UserEvent::CloseRequest => {
                    let _ = webview.evaluate_script("window.__tryClose&&window.__tryClose()");
                }
                UserEvent::Banner(on) => {
                    if mode == Mode::Home {
                        window.set_inner_size(LogicalSize::new(w, if on { h + 56.0 } else { h }));
                    }
                }
            },
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if mode.asks_before_close() {
                    let _ = webview.evaluate_script("window.__tryClose&&window.__tryClose()");
                } else {
                    if matches!(mode, Mode::Launch(_)) {
                        progress::cancel();
                    }
                    *flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });

    match mode {
        Mode::Launch(_) => progress::snapshot().finished.unwrap_or(error_code::CANCELLED),
        _ => EXIT_CODE.load(Ordering::SeqCst),
    }
}

fn center(window: &tao::window::Window) {
    if let Some(m) = window.current_monitor().or_else(|| window.primary_monitor()) {
        let ms = m.size();
        let ws = window.outer_size();
        let mp = m.position();
        let x = mp.x + (ms.width as i32 - ws.width as i32) / 2;
        let y = mp.y + (ms.height as i32 - ws.height as i32) / 2;
        window.set_outer_position(PhysicalPosition::new(x, y));
    }
}

/// No window available (WebView2 missing, ...): still do what the user asked.
fn fallback(mode: Mode) -> i32 {
    if let Mode::Launch(lm) = mode {
        return Bootstrapper::new(lm).run();
    }
    1
}

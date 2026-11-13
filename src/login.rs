//! The "Sign in with Roblox" window (`ruststrap -accountlogin <file>`). It is its own process because a
//! process can only use one WebView2 profile, and this one must start empty every time: no other
//! account's session can leak in. When Roblox sets the `.ROBLOSECURITY` cookie the window closes and
//! the cookie, protected with DPAPI, is written to `<file>` for the settings window to pick up.

use std::fs;
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::{WebContext, WebView, WebViewBuilder};

use crate::app::error_code;
use crate::paths;

static CODE: AtomicI32 = AtomicI32::new(error_code::CANCELLED);

enum Tick {
    Poll,
}

fn roblosecurity(webview: &WebView) -> Option<String> {
    let cookies = webview.cookies_for_url("https://www.roblox.com/").ok()?;
    cookies.iter().find(|c| c.name() == ".ROBLOSECURITY").map(|c| c.value().to_string()).filter(|v| !v.is_empty())
}

pub fn run(out_file: &str) -> i32 {
    const IDENT: &str = "Login::Window";
    let dir = paths::get().base.join("WebView2-login");
    let _ = fs::remove_dir_all(&dir);

    let mut event_loop = EventLoopBuilder::<Tick>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = match WindowBuilder::new()
        .with_title("Roblox")
        .with_inner_size(LogicalSize::new(520.0, 720.0))
        .with_window_icon(crate::ui::web::window_icon())
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            crate::log!(IDENT, "Window failed: {e}");
            return 1;
        }
    };

    let mut ctx = WebContext::new(Some(dir.clone()));
    let webview = match WebViewBuilder::new_with_web_context(&mut ctx).with_url("https://www.roblox.com/login").build(&window) {
        Ok(v) => v,
        Err(e) => {
            crate::log!(IDENT, "WebView2 failed: {e}");
            return 1;
        }
    };

    // look for the cookie once a second until the window is gone
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(1000));
        if proxy.send_event(Tick::Poll).is_err() {
            break;
        }
    });

    let out = out_file.to_string();
    event_loop.run_return(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Tick::Poll) => {
                if let Some(cookie) = roblosecurity(&webview) {
                    if let Ok(enc) = crate::accounts::protect(&cookie) {
                        if fs::write(&out, enc).is_ok() {
                            CODE.store(error_code::SUCCESS, Ordering::SeqCst);
                            *flow = ControlFlow::Exit;
                        }
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });

    let _ = fs::remove_dir_all(&dir);
    CODE.load(Ordering::SeqCst)
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Ferrum — a native web browser written in Rust.
//!
//! * Pages: the system's web engine through wry — WebKit on macOS, WebView2
//!   (Chromium) on Windows, WebKitGTK on Linux.
//! * Browser interface (tabs, toolbar, address bar, menus, panels): drawn
//!   natively in Rust with egui. There is no HTML/CSS/JS in the app itself.

// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adblock;
mod blocker;
mod bookmarks_io;
mod browser;
mod browser_import;
mod devtools;
mod downloads;
mod features_ui;
mod find;
mod gl;
mod icons;
mod location;
mod notify;
mod overlay;
mod overlay_ui;
mod pagescript;
mod profile;
mod reader;
mod settings_page;
mod theme;
mod ui;
mod updates;

use std::error::Error;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::browser::Browser;

/// Ferrum's version. Release builds get it from CI (`FERRUM_VERSION`), so the
/// app, its download and its release page agree; local builds use Cargo.toml's.
pub const VERSION: &str = match option_env!("FERRUM_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Start-up timing: run with `FERRUM_TIMING=1` to print how long each phase takes.
pub fn timing(label: &str) {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    static ENABLED: OnceLock<bool> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    if *ENABLED.get_or_init(|| std::env::var_os("FERRUM_TIMING").is_some()) {
        eprintln!("[ferrum {:>7.1} ms] {label}", start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Sent from other threads (downloads, the engine's callbacks) when the main thread has work.
#[derive(Debug, Clone, Copy)]
pub struct WakeUp;

/// Ferrum's own connections (downloads, updates, block list, favicons) trust
/// the certificates the operating system trusts, as the web engine does, so
/// they also work on networks that add their own certificate (schools, offices).
pub fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build()
}

struct Args {
    url: Option<String>,
    private: bool,
}

fn parse_args() -> Args {
    let mut args = Args { url: None, private: false };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--private" | "-p" => args.private = true,
            "--help" | "-h" => {
                println!("Usage: ferrum [--private] [URL]");
                std::process::exit(0);
            },
            "--version" | "-V" => {
                println!("ferrum {VERSION}");
                std::process::exit(0);
            },
            other if !other.starts_with('-') => args.url = Some(other.to_string()),
            _ => {},
        }
    }
    args
}

enum App {
    Starting { proxy: EventLoopProxy<WakeUp>, args: Option<Args> },
    Running(Box<Browser>),
    Done,
}

impl ApplicationHandler<WakeUp> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let App::Starting { proxy, args } = self {
            let args = args.take().unwrap_or(Args { url: None, private: false });
            let browser = Browser::new(event_loop, proxy.clone(), args.url, args.private);
            *self = App::Running(Box::new(browser));
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: WakeUp) {
        if let App::Running(browser) = self {
            browser.pump();
            browser.window.request_redraw();
        }
    }

    /// Sleep until there is input, a wake-up from another thread, or the next
    /// scheduled interface repaint — never spin.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Linux: WebKitGTK runs on GTK's main loop; give it its turn.
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        let App::Running(browser) = self else { return };
        browser.pump();
        match browser.next_wake() {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let App::Running(browser) = self else { return };
        if !browser.handle_window_event(event_loop, id, event) {
            *self = App::Done;
            event_loop.exit();
        }
    }
}

/// Where crash reports go: `<data dir>/Ferrum/crash.log`.
pub fn crash_log_path() -> std::path::PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Ferrum").join("crash.log")
}

/// On a crash, write the error and a backtrace to the crash log and, on the
/// main thread, show it in a dialog, so crashes are never silent.
fn install_crash_reporter() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed").to_string();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let report = format!(
            "Ferrum {} crashed\nOS: {} {}\nThread: {thread_name}\nError: {message}\nAt: {location}\n\n{}\n",
            VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture(),
        );
        let path = crash_log_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, &report);
        // Only interrupt the user for main-thread crashes.
        if thread_name == "main" {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Ferrum crashed")
                .set_description(format!("{message}\n\nAt {location}\n\nA full report was saved to:\n{}", path.display()))
                .show();
        }
    }));
}

fn main() -> Result<(), Box<dyn Error>> {
    timing("process start");
    install_crash_reporter();
    updates::clean_up_after_update();
    let args = parse_args();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install TLS crypto provider");

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Linux: wry embeds WebKitGTK views as X11 child windows, so use X11
    // (XWayland on Wayland desktops), and start GTK first.
    #[cfg(target_os = "linux")]
    let event_loop = {
        // SAFETY: set before any other thread starts.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
        gtk::init().map_err(|e| format!("Could not start GTK: {e}"))?;
        // WebKitGTK's GL use trips harmless GLXBadWindow errors (170) that
        // winit would otherwise treat as fatal.
        winit::platform::x11::register_xlib_error_hook(Box::new(|_display, error| {
            let error = error as *mut x11_dl::xlib::XErrorEvent;
            unsafe { (*error).error_code == 170 }
        }));
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoop::<WakeUp>::with_user_event().with_x11().build()?
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::<WakeUp>::with_user_event().build()?;
    let mut app = App::Starting { proxy: event_loop.create_proxy(), args: Some(args) };
    event_loop.run_app(&mut app)?;
    Ok(())
}

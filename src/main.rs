// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Ferrum — a native web browser written in Rust.
//!
//! * Page engine: Servo (Rust; the project Firefox's Stylo CSS engine and
//!   WebRender came from).
//! * Browser interface (tabs, toolbar, address bar, menus, panels, dialogs):
//!   drawn natively with egui on the same OpenGL surface. There is no
//!   HTML/CSS/JS in the app itself.

// No console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod browser;
mod downloads;
mod find;
mod icons;
mod keys;
mod location;
mod print;
mod profile;
mod ui;

use std::error::Error;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::browser::Browser;

/// Sent from other threads (Servo, downloads) when the main thread has work.
#[derive(Debug, Clone, Copy)]
pub struct WakeUp;

#[derive(Clone)]
struct Waker(EventLoopProxy<WakeUp>);

impl servo::EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        let _ = self.0.send_event(WakeUp);
    }
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
                println!("ferrum {}", env!("CARGO_PKG_VERSION"));
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
            let waker = Box::new(Waker(proxy.clone()));
            let browser = Browser::new(event_loop, proxy.clone(), waker, args.url, args.private);
            *self = App::Running(Box::new(browser));
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: WakeUp) {
        if let App::Running(browser) = self {
            browser.pump();
        }
    }

    /// Sleep until there is input, a wake-up from another thread, or the next
    /// scheduled interface repaint — never spin.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let App::Running(browser) = self else { return };
        match browser.next_wake() {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let App::Running(browser) = self else { return };
        if !browser.handle_window_event(event) {
            *self = App::Done;
            event_loop.exit();
        }
    }
}

/// Where crash reports go: `<data dir>/Ferrum/crash.log`.
fn crash_log_path() -> std::path::PathBuf {
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
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture(),
        );
        let path = crash_log_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, &report);
        // Servo's background threads may panic during normal shutdown; only
        // interrupt the user for main-thread crashes.
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
    install_crash_reporter();
    let args = parse_args();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install TLS crypto provider");

    let event_loop = EventLoop::<WakeUp>::with_user_event().build()?;
    let mut app = App::Starting { proxy: event_loop.create_proxy(), args: Some(args) };
    event_loop.run_app(&mut app)?;
    Ok(())
}

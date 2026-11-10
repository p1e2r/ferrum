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
mod icons;
mod keys;
mod location;
mod profile;
mod ui;

use std::error::Error;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
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

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let App::Running(browser) = self else { return };
        if !browser.handle_window_event(event) {
            *self = App::Done;
            event_loop.exit();
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install TLS crypto provider");

    let event_loop = EventLoop::<WakeUp>::with_user_event().build()?;
    let mut app = App::Starting { proxy: event_loop.create_proxy(), args: Some(args) };
    event_loop.run_app(&mut app)?;
    Ok(())
}

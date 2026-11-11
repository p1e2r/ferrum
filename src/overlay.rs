// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! Popups that float over the page.
//!
//! The page is a native view drawn by the system's web engine, above
//! anything Ferrum paints in the main window. So the menu, address-bar
//! suggestions, site information, the tab menu, toasts and notifications
//! each get a small borderless window of their own, placed over the main
//! window (Chrome does the same). They share the main window's OpenGL
//! context and are drawn with their own egui context.
//!
//! Each frame, the main interface says which popups it wants and where
//! (`Browser::overlay_requests`); `Overlays::sync` creates, moves and closes
//! the windows to match, then `draw` paints each one.

use std::sync::Arc;

use egui_glow::EguiGlow;
use glutin::surface::{Surface, WindowSurface};
use raw_window_handle::HasWindowHandle;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

use crate::gl::GlWindow;
use crate::profile::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    Menu,
    Suggestions,
    SiteInfo,
    TabMenu,
    Toast,
    Notification,
    SearchBlocked,
    Permission,
}

impl OverlayKind {
    /// Popups the user clicks in (the others only show something).
    fn interactive(self) -> bool {
        !matches!(self, OverlayKind::Toast)
    }
}

struct Overlay {
    kind: OverlayKind,
    // Field order matters: egui's GL resources and the surface go before the window.
    egui: Option<EguiGlow>,
    surface: Surface<WindowSurface>,
    window: Window,
    /// Where the main interface wants it, in main-window points.
    rect: egui::Rect,
    /// The size its contents took last time, in points.
    content: Option<egui::Vec2>,
    placed: Option<(i32, i32, u32, u32)>,
    style_key: u64,
    /// Popups start hidden and appear once their size is known, so they
    /// never flash at the wrong size.
    visible: bool,
}

#[derive(Default)]
pub struct Overlays {
    list: Vec<Overlay>,
    /// The pointer is over one of the popups.
    pub pointer_inside: bool,
}

impl Overlays {
    pub fn kinds(&self) -> Vec<OverlayKind> {
        self.list.iter().map(|o| o.kind).collect()
    }

    /// Make the open popups match `requests`.
    pub fn sync(
        &mut self,
        event_loop: &ActiveEventLoop,
        gl_window: &GlWindow,
        gl: &Arc<glow::Context>,
        requests: &[(OverlayKind, egui::Rect)],
        ppp: f32,
        settings: &Settings,
    ) {
        // Close popups that are no longer wanted.
        let mut i = 0;
        while i < self.list.len() {
            if requests.iter().any(|(k, _)| *k == self.list[i].kind) {
                i += 1;
            } else {
                let mut overlay = self.list.remove(i);
                gl_window.make_current_on(&overlay.surface);
                if let Some(mut egui) = overlay.egui.take() {
                    egui.destroy();
                }
            }
        }
        if !self.list.iter().any(|o| o.kind.interactive()) {
            self.pointer_inside = false;
        }

        let parent = &gl_window.window;
        let key = crate::ui::style_key(settings);
        for (kind, rect) in requests {
            if !self.list.iter().any(|o| o.kind == *kind) {
                match create(event_loop, gl_window, gl, parent, *rect, ppp) {
                    Some((window, surface, egui)) => {
                        crate::ui::style(&egui.egui_ctx, settings);
                        self.list.push(Overlay { kind: *kind, egui: Some(egui), surface, window, rect: *rect, content: None, placed: None, style_key: key, visible: false });
                    },
                    None => continue,
                }
            }
            let Some(overlay) = self.list.iter_mut().find(|o| o.kind == *kind) else { continue };
            if overlay.style_key != key {
                overlay.style_key = key;
                if let Some(egui) = &overlay.egui {
                    crate::ui::style(&egui.egui_ctx, settings);
                }
            }
            overlay.rect = *rect;
            // Fit the window to its contents (measured last frame), up to the requested size.
            let size = match overlay.content {
                Some(c) => egui::vec2(rect.width(), c.y.min(rect.height()).max(8.0)),
                None => rect.size(),
            };
            let origin = window_origin(parent);
            let placed = (
                origin.0 + (rect.left() * ppp).round() as i32,
                origin.1 + (rect.top() * ppp).round() as i32,
                (size.x * ppp).round().max(1.0) as u32,
                (size.y * ppp).round().max(1.0) as u32,
            );
            if overlay.placed != Some(placed) {
                overlay.placed = Some(placed);
                overlay.window.set_outer_position(winit::dpi::PhysicalPosition::new(placed.0, placed.1));
                let physical = winit::dpi::PhysicalSize::new(placed.2, placed.3);
                let _ = overlay.window.request_inner_size(physical);
                gl_window.resize_surface(&overlay.surface, physical);
            }
            if overlay.content.is_some() && !overlay.visible {
                overlay.visible = true;
                overlay.window.set_visible(true);
                gl_window.window.request_redraw();
            }
        }
    }

    /// Paint one popup. `contents` draws it; the popup frame is drawn around it.
    pub fn draw(&mut self, kind: OverlayKind, gl_window: &GlWindow, contents: impl FnMut(&mut egui::Ui)) {
        let Some(overlay) = self.list.iter_mut().find(|o| o.kind == kind) else { return };
        let Some(mut egui) = overlay.egui.take() else { return };
        gl_window.make_current_on(&overlay.surface);
        let mut contents = contents;
        let mut used = None;
        let width = overlay.rect.width();
        egui.run(&overlay.window, |ui| {
            let frame = egui::Frame::new()
                .fill(ui.visuals().window_fill)
                .stroke(ui.visuals().window_stroke)
                .inner_margin(egui::Margin::same(if kind == OverlayKind::Toast { 0 } else { 6 }));
            let response = frame.show(ui, |ui| {
                ui.set_width(width - 14.0);
                contents(ui);
            });
            used = Some(response.response.rect.size());
        });
        egui.paint(&overlay.window);
        gl_window.swap(&overlay.surface);
        if let Some(size) = used {
            if overlay.content.is_none_or(|c| (c.y - size.y).abs() > 0.5) {
                overlay.content = Some(size);
                // The size changed: draw again at the new size.
                gl_window.window.request_redraw();
            }
        }
        overlay.egui = Some(egui);
    }

    /// Input for a popup window. Returns true if the window was one of ours.
    pub fn handle_event(&mut self, id: WindowId, event: &WindowEvent, main: &Window) -> bool {
        let Some(overlay) = self.list.iter_mut().find(|o| o.window.id() == id) else { return false };
        match event {
            WindowEvent::CursorEntered { .. } => self.pointer_inside = true,
            WindowEvent::CursorLeft { .. } => self.pointer_inside = false,
            WindowEvent::RedrawRequested => {},
            _ => {},
        }
        if let Some(egui) = overlay.egui.as_mut() {
            let response = egui.on_window_event(&overlay.window, event);
            if response.repaint || matches!(event, WindowEvent::RedrawRequested) {
                main.request_redraw();
            }
        }
        true
    }

    pub fn destroy(&mut self) {
        for overlay in &mut self.list {
            if let Some(mut egui) = overlay.egui.take() {
                egui.destroy();
            }
        }
        self.list.clear();
    }
}

/// Popup windows are positioned relative to the main window's content on
/// Windows and Linux (they're child windows), and on screen on macOS.
fn window_origin(parent: &Window) -> (i32, i32) {
    if cfg!(target_os = "macos") {
        parent.inner_position().map(|p| (p.x, p.y)).unwrap_or((0, 0))
    } else {
        (0, 0)
    }
}

fn create(
    event_loop: &ActiveEventLoop,
    gl_window: &GlWindow,
    gl: &Arc<glow::Context>,
    parent: &Window,
    rect: egui::Rect,
    ppp: f32,
) -> Option<(Window, Surface<WindowSurface>, EguiGlow)> {
    let origin = window_origin(parent);
    let parent_handle = parent.window_handle().ok()?.as_raw();
    let attributes = Window::default_attributes()
        .with_title("Ferrum popup")
        .with_decorations(false)
        .with_resizable(false)
        .with_active(false)
        .with_visible(false)
        .with_window_level(WindowLevel::Normal)
        .with_position(winit::dpi::PhysicalPosition::new(origin.0 + (rect.left() * ppp) as i32, origin.1 + (rect.top() * ppp) as i32))
        .with_inner_size(winit::dpi::PhysicalSize::new((rect.width() * ppp).max(1.0) as u32, (rect.height() * ppp).max(1.0) as u32));
    // SAFETY: the parent window outlives its popups (they're closed first).
    let attributes = unsafe { attributes.with_parent_window(Some(parent_handle)) };
    let (window, surface) = gl_window.create_popup(event_loop, attributes)?;
    gl_window.make_current_on(&surface);
    let egui = EguiGlow::new(event_loop, gl.clone(), None, None, false);
    Some((window, surface, egui))
}

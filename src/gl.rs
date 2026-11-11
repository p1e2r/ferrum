// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The window and its OpenGL surface, which Ferrum's interface is drawn on.
//! Web pages are separate native views placed on top of it (see browser.rs).

use std::ffi::CString;
use std::num::NonZeroU32;
use std::rc::Rc;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow as _};
use raw_window_handle::HasWindowHandle;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

pub struct GlWindow {
    pub window: Rc<Window>,
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    config: glutin::config::Config,
}

impl GlWindow {
    /// Create the window with an OpenGL context, and a `glow` handle for egui.
    pub fn new(event_loop: &ActiveEventLoop, attributes: WindowAttributes) -> (Self, glow::Context) {
        let template = ConfigTemplateBuilder::new().prefer_hardware_accelerated(Some(true)).with_depth_size(0).with_stencil_size(0);
        let (window, config) = DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(attributes))
            .build(event_loop, template, |mut configs| configs.next().expect("an OpenGL configuration"))
            .expect("Could not create the window with OpenGL");
        let window = window.expect("window");
        let display = config.display();
        let raw = window.window_handle().ok().map(|h| h.as_raw());
        let attributes = ContextAttributesBuilder::new().build(raw);
        let fallback = ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(None)).build(raw);
        let not_current = unsafe {
            display
                .create_context(&config, &attributes)
                .or_else(|_| display.create_context(&config, &fallback))
                .expect("Could not create an OpenGL context")
        };
        let surface_attributes = window.build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new()).expect("surface attributes");
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes).expect("OpenGL surface") };
        let context = not_current.make_current(&surface).expect("make the OpenGL context current");
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN));
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).expect("GL function name");
                display.get_proc_address(&name)
            })
        };
        (Self { window: Rc::new(window), context, surface, config }, gl)
    }

    /// A small borderless window above this one, sharing its OpenGL context,
    /// for popups that must float over the page (see overlay.rs).
    pub fn create_popup(&self, event_loop: &ActiveEventLoop, attributes: WindowAttributes) -> Option<(Window, Surface<WindowSurface>)> {
        let window = glutin_winit::finalize_window(event_loop, attributes, &self.config).ok()?;
        let surface_attributes = window.build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new()).ok()?;
        let surface = unsafe { self.config.display().create_window_surface(&self.config, &surface_attributes).ok()? };
        Some((window, surface))
    }

    pub fn make_current_on(&self, surface: &Surface<WindowSurface>) {
        let _ = glutin::context::PossiblyCurrentGlContext::make_current(&self.context, surface);
    }

    pub fn swap(&self, surface: &Surface<WindowSurface>) {
        let _ = surface.swap_buffers(&self.context);
    }

    pub fn resize_surface(&self, surface: &Surface<WindowSurface>, size: winit::dpi::PhysicalSize<u32>) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            surface.resize(&self.context, w, h);
        }
    }

    pub fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            self.surface.resize(&self.context, w, h);
        }
    }

    pub fn make_current(&self) {
        let _ = glutin::context::PossiblyCurrentGlContext::make_current(&self.context, &self.surface);
    }

    pub fn swap_buffers(&self) {
        let _ = self.surface.swap_buffers(&self.context);
    }
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The browser window: Servo webviews plus a native egui interface on one GL surface.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use egui_glow::EguiGlow;
use euclid::Scale;
use servo::{
    AuthenticationRequest, ColorPicker, ContextMenu, CookieSource, CreateNewWebViewRequest,
    Cursor, DevicePoint, EmbedderControl, EmbedderControlId, EventLoopWaker, FilePicker,
    InputEvent, LoadStatus, MouseButton as ServoMouseButton, MouseButtonAction, MouseButtonEvent,
    MouseLeftViewportEvent, MouseMoveEvent, NavigationRequest, OffscreenRenderingContext, Opts,
    PermissionRequest, PixelFormat, RenderingContext, SelectElement, Servo, ServoBuilder,
    SimpleDialog, StorageType, WebView, WebViewBuilder, WebViewDelegate, WebViewId, WheelDelta,
    WheelEvent, WheelMode, WindowRenderingContext,
};
use url::Url;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{CursorIcon, Fullscreen, Window};

use crate::WakeUp;
use crate::downloads::{Downloads, looks_like_download};
use crate::keys::keyboard_event;
use crate::location::input_to_url;
use crate::profile::{Profile, ThemeChoice};

pub const APP_NAME: &str = "Ferrum";
const ZOOM_STEPS: [f32; 13] = [0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

// ---------------------------------------------------------------------------
// Servo -> browser callbacks
// ---------------------------------------------------------------------------

/// Something a page asked the browser to show.
pub enum PageControl {
    Dialog { dialog: SimpleDialog, input: String },
    Auth { request: AuthenticationRequest, user: String, pass: String },
    Permission(PermissionRequest),
    Select(SelectElement),
    Color { picker: ColorPicker, color: egui::Color32 },
    ContextMenu(ContextMenu),
}

pub struct Pending {
    pub webview: WebViewId,
    pub id: Option<EmbedderControlId>,
    pub control: PageControl,
}

enum Event {
    Opened(WebView),
    Closed(WebViewId),
    Control(Pending),
    HideControl(EmbedderControlId),
    FilePicker(FilePicker),
    Crashed(WebViewId, String),
    Fullscreen(bool),
    Loaded(WebView),
    Title(WebView),
    Favicon(WebViewId),
    Download(Url, Option<Url>),
}

/// Receives callbacks from Servo for every webview. Callbacks only queue work
/// (handled in `Browser::pump`) or ask for a redraw, so they stay cheap.
struct Delegate {
    window: Rc<Window>,
    rendering_context: Rc<OffscreenRenderingContext>,
    events: RefCell<Vec<Event>>,
}

impl Delegate {
    fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
        self.window.request_redraw();
    }
}

impl WebViewDelegate for Delegate {
    fn notify_new_frame_ready(&self, _: WebView) {
        self.window.request_redraw();
    }
    fn notify_url_changed(&self, _: WebView, _: Url) {
        self.window.request_redraw();
    }
    fn notify_page_title_changed(&self, webview: WebView, _: Option<String>) {
        self.push(Event::Title(webview));
    }
    fn notify_status_text_changed(&self, _: WebView, _: Option<String>) {
        self.window.request_redraw();
    }
    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        if status == LoadStatus::Complete {
            self.push(Event::Loaded(webview));
        } else {
            self.window.request_redraw();
        }
    }
    fn notify_history_changed(&self, _: WebView, _: Vec<Url>, _: usize) {
        self.window.request_redraw();
    }
    fn notify_favicon_changed(&self, webview: WebView) {
        self.push(Event::Favicon(webview.id()));
    }
    fn notify_cursor_changed(&self, webview: WebView, cursor: Cursor) {
        if webview.focused() {
            set_cursor(&self.window, cursor);
        }
    }
    fn notify_fullscreen_state_changed(&self, _: WebView, fullscreen: bool) {
        self.push(Event::Fullscreen(fullscreen));
    }
    fn notify_closed(&self, webview: WebView) {
        self.push(Event::Closed(webview.id()));
    }
    fn notify_crashed(&self, webview: WebView, reason: String, _: Option<String>) {
        self.push(Event::Crashed(webview.id(), reason));
    }

    /// Pages that open new windows (target=_blank, window.open) get a new tab.
    fn request_create_new(&self, parent: WebView, request: CreateNewWebViewRequest) {
        let webview = request
            .builder(self.rendering_context.clone())
            .hidpi_scale_factor(Scale::new(self.window.scale_factor() as f32))
            .delegate(parent.delegate())
            .build();
        self.push(Event::Opened(webview));
    }

    /// Links to files (zip, exe, pdf…) become downloads instead of navigations.
    fn request_navigation(&self, webview: WebView, request: NavigationRequest) {
        if matches!(request.url.scheme(), "http" | "https") && looks_like_download(&request.url) {
            let url = request.url.clone();
            request.deny();
            self.push(Event::Download(url, webview.url()));
        } else {
            request.allow();
        }
    }

    fn request_permission(&self, webview: WebView, request: PermissionRequest) {
        self.push(Event::Control(Pending {
            webview: webview.id(),
            id: None,
            control: PageControl::Permission(request),
        }));
    }

    fn request_authentication(&self, webview: WebView, request: AuthenticationRequest) {
        self.push(Event::Control(Pending {
            webview: webview.id(),
            id: None,
            control: PageControl::Auth { request, user: String::new(), pass: String::new() },
        }));
    }

    fn show_embedder_control(&self, webview: WebView, control: EmbedderControl) {
        let id = Some(control.id());
        let control = match control {
            EmbedderControl::SelectElement(select) => PageControl::Select(select),
            EmbedderControl::ColorPicker(picker) => {
                let color = picker
                    .current_color()
                    .map(|c| egui::Color32::from_rgb(c.red, c.green, c.blue))
                    .unwrap_or(egui::Color32::BLACK);
                PageControl::Color { picker, color }
            },
            EmbedderControl::FilePicker(picker) => {
                self.push(Event::FilePicker(picker));
                return;
            },
            EmbedderControl::SimpleDialog(dialog) => {
                let input = match &dialog {
                    SimpleDialog::Prompt(p) => p.current_value().to_string(),
                    _ => String::new(),
                };
                PageControl::Dialog { dialog, input }
            },
            EmbedderControl::ContextMenu(menu) => PageControl::ContextMenu(menu),
            // Desktop keyboards don't need an on-screen input method.
            EmbedderControl::InputMethod(_) => return,
        };
        self.push(Event::Control(Pending { webview: webview.id(), id, control }));
    }

    fn hide_embedder_control(&self, _: WebView, id: EmbedderControlId) {
        self.push(Event::HideControl(id));
    }
}

fn set_cursor(window: &Window, cursor: Cursor) {
    let icon = match cursor {
        Cursor::None => {
            window.set_cursor_visible(false);
            return;
        },
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Help => CursorIcon::Help,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::EwResize | Cursor::EResize | Cursor::WResize | Cursor::ColResize => CursorIcon::EwResize,
        Cursor::NsResize | Cursor::NResize | Cursor::SResize | Cursor::RowResize => CursorIcon::NsResize,
        Cursor::NeswResize | Cursor::NeResize | Cursor::SwResize => CursorIcon::NeswResize,
        Cursor::NwseResize | Cursor::NwResize | Cursor::SeResize => CursorIcon::NwseResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        _ => CursorIcon::Default,
    };
    window.set_cursor(icon);
    window.set_cursor_visible(true);
}

// ---------------------------------------------------------------------------
// Browser state
// ---------------------------------------------------------------------------

/// A tab is either a live Servo webview or the native start page.
pub struct Tab {
    pub webview: Option<WebView>,
    pub crashed: Option<String>,
    pub zoom: f32,
    pub favicon: Option<egui::TextureHandle>,
    pub favicon_dirty: bool,
}

impl Tab {
    fn new(webview: Option<WebView>, zoom: f32) -> Self {
        Self { webview, crashed: None, zoom, favicon: None, favicon_dirty: false }
    }

    pub fn title(&self) -> String {
        match &self.webview {
            Some(w) => w
                .page_title()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| w.url().and_then(|u| u.host_str().map(str::to_owned)))
                .unwrap_or_else(|| "Loading…".to_string()),
            None => "New Tab".to_string(),
        }
    }

    pub fn url(&self) -> Option<Url> {
        self.webview.as_ref().and_then(|w| w.url())
    }

    pub fn loading(&self) -> bool {
        self.webview.as_ref().is_some_and(|w| w.load_status() != LoadStatus::Complete)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidePanel {
    None,
    History,
    Bookmarks,
    Downloads,
    Settings,
}

pub struct Browser {
    pub(crate) window: Rc<Window>,
    window_context: Rc<WindowRenderingContext>,
    pub(crate) page_context: Rc<OffscreenRenderingContext>,
    pub(crate) servo: Servo,
    delegate: Rc<Delegate>,
    egui: Option<EguiGlow>,

    pub(crate) profile: Profile,
    pub(crate) downloads: Downloads,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    pub(crate) controls: Vec<Pending>,
    closed_tabs: Vec<String>,

    // Address bar
    pub(crate) location: String,
    pub(crate) editing: bool,
    pub(crate) focus_location: bool,
    pub(crate) suggestion_index: Option<usize>,
    pub(crate) start_query: String,

    // Chrome state
    pub(crate) panel: SidePanel,
    pub(crate) menu_open: bool,
    pub(crate) history_filter: String,
    pub(crate) page_fullscreen: bool,
    pub(crate) window_fullscreen: bool,
    pub(crate) toast: Option<(String, Instant)>,
    page_theme: Option<servo::Theme>,

    // Page geometry (egui points) and input
    pub(crate) content_rect: egui::Rect,
    pub(crate) ppp: f32,
    mouse: Option<(f32, f32)>,
    pub(crate) modifiers: ModifiersState,
}

impl Browser {
    pub fn new(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<WakeUp>,
        waker: Box<dyn EventLoopWaker>,
        initial_url: Option<String>,
        private: bool,
    ) -> Self {
        let profile = Profile::load(if private { None } else { Profile::default_dir() });

        let title = if private { format!("{APP_NAME} (Private)") } else { APP_NAME.to_string() };
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 840.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(480.0, 320.0));
        let window = Rc::new(event_loop.create_window(attributes).expect("create window"));

        let display_handle = event_loop.display_handle().expect("display handle");
        let window_handle = window.window_handle().expect("window handle");
        let window_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                .expect("Could not create an OpenGL context for the window"),
        );
        // Pages render into an offscreen framebuffer that egui composites under the toolbar.
        let page_context = Rc::new(window_context.offscreen_context(window.inner_size()));
        page_context.make_current().expect("make GL context current");

        // Cookies and site storage persist in the profile; private windows keep them in memory.
        let mut opts = Opts::default();
        match profile.engine_dir() {
            Some(dir) => {
                let _ = std::fs::create_dir_all(&dir);
                opts.config_dir = Some(dir);
            },
            None => opts.temporary_storage = true,
        }
        let servo = ServoBuilder::default().opts(opts).event_loop_waker(waker).build();
        servo.setup_logging();

        let egui = EguiGlow::new(event_loop, page_context.glow_gl_api(), None, None, false);
        crate::ui::style(&egui.egui_ctx, profile.settings.theme);

        let delegate = Rc::new(Delegate {
            window: window.clone(),
            rendering_context: page_context.clone(),
            events: Default::default(),
        });

        let notify_window = proxy.clone();
        let downloads = Downloads::new(std::sync::Arc::new(move || {
            let _ = notify_window.send_event(WakeUp);
        }));

        let mut browser = Self {
            window,
            window_context,
            page_context,
            servo,
            delegate,
            egui: Some(egui),
            profile,
            downloads,
            tabs: Vec::new(),
            active: 0,
            controls: Vec::new(),
            closed_tabs: Vec::new(),
            location: String::new(),
            editing: false,
            focus_location: false,
            suggestion_index: None,
            start_query: String::new(),
            panel: SidePanel::None,
            menu_open: false,
            history_filter: String::new(),
            page_fullscreen: false,
            window_fullscreen: false,
            toast: None,
            page_theme: None,
            content_rect: egui::Rect::NOTHING,
            ppp: 1.0,
            mouse: None,
            modifiers: ModifiersState::empty(),
        };

        let engine = browser.profile.settings.search_engine;
        if let Some(url) = initial_url.as_deref().and_then(|u| input_to_url(u, engine)) {
            browser.open_tab(Some(url));
        } else if browser.profile.settings.restore_session && !browser.profile.is_private() {
            let (urls, active) = browser.profile.load_session();
            for url in urls.iter().filter_map(|u| Url::parse(u).ok()) {
                browser.open_tab(Some(url));
            }
            if !browser.tabs.is_empty() {
                browser.activate(active);
            }
        }
        if browser.tabs.is_empty() {
            browser.open_home_tab();
        }
        browser
    }

    // ----- tabs -------------------------------------------------------------

    pub(crate) fn create_webview(&self, url: Url) -> WebView {
        let webview = WebViewBuilder::new(&self.servo, self.page_context.clone())
            .url(url)
            .hidpi_scale_factor(Scale::new(self.window.scale_factor() as f32))
            .delegate(self.delegate.clone())
            .build();
        if let Some(theme) = self.page_theme {
            webview.notify_theme_change(theme);
        }
        let zoom = self.profile.settings.default_zoom;
        if (zoom - 1.0).abs() > f32::EPSILON {
            webview.set_page_zoom(zoom);
        }
        webview
    }

    fn home_url(&self) -> Option<Url> {
        let home = self.profile.settings.home_page.trim();
        if home.is_empty() {
            None
        } else {
            input_to_url(home, self.profile.settings.search_engine)
        }
    }

    pub(crate) fn open_home_tab(&mut self) {
        let home = self.home_url();
        self.open_tab(home);
        if self.tabs[self.active].webview.is_none() {
            self.focus_location = true;
        }
    }

    pub(crate) fn open_tab(&mut self, url: Option<Url>) {
        let webview = url.map(|url| self.create_webview(url));
        self.tabs.push(Tab::new(webview, self.profile.settings.default_zoom));
        self.activate(self.tabs.len() - 1);
    }

    pub(crate) fn open_background_tab(&mut self, url: Url) {
        let webview = self.create_webview(url);
        webview.hide();
        webview.set_focused(false);
        self.tabs.insert(self.active + 1, Tab::new(Some(webview), self.profile.settings.default_zoom));
        self.toast("Opened in a new tab");
    }

    pub(crate) fn activate(&mut self, index: usize) {
        self.active = index.min(self.tabs.len().saturating_sub(1));
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(webview) = &tab.webview {
                if i == self.active {
                    webview.show();
                    webview.set_focused(true);
                } else {
                    // Hidden tabs are throttled by Servo, saving CPU and battery.
                    webview.set_focused(false);
                    webview.hide();
                }
            }
        }
        self.editing = false;
        self.suggestion_index = None;
        self.sync_location();
        self.window.request_redraw();
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        if let Some(url) = tab.url() {
            self.closed_tabs.push(url.to_string());
            if self.closed_tabs.len() > 25 {
                self.closed_tabs.remove(0);
            }
        }
        if let Some(webview) = &tab.webview {
            self.controls.retain(|c| c.webview != webview.id());
        }
        if self.tabs.is_empty() {
            self.open_home_tab();
            return;
        }
        let next = if index < self.active || (index == self.active && self.active == self.tabs.len()) {
            self.active.saturating_sub(1)
        } else {
            self.active
        };
        self.activate(next);
    }

    fn reopen_closed_tab(&mut self) {
        if let Some(url) = self.closed_tabs.pop().and_then(|u| Url::parse(&u).ok()) {
            self.open_tab(Some(url));
        }
    }

    pub(crate) fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub(crate) fn active_webview(&self) -> Option<&WebView> {
        self.tabs.get(self.active)?.webview.as_ref()
    }

    pub(crate) fn tab_index(&self, id: WebViewId) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview.as_ref().is_some_and(|w| w.id() == id))
    }

    pub(crate) fn navigate_input(&mut self, input: &str) {
        if let Some(url) = input_to_url(input, self.profile.settings.search_engine) {
            self.navigate(url);
        }
    }

    pub(crate) fn navigate(&mut self, url: Url) {
        self.editing = false;
        self.suggestion_index = None;
        let tab = &mut self.tabs[self.active];
        tab.crashed = None;
        match &tab.webview {
            Some(webview) => webview.load(url),
            None => {
                let webview = self.create_webview(url);
                webview.set_focused(true);
                self.tabs[self.active].webview = Some(webview);
            },
        }
        self.sync_location();
    }

    pub(crate) fn go_home(&mut self) {
        match self.home_url() {
            Some(url) => self.navigate(url),
            None => {
                self.tabs[self.active] = Tab::new(None, self.profile.settings.default_zoom);
                self.activate(self.active);
                self.focus_location = true;
            },
        }
    }

    pub(crate) fn sync_location(&mut self) {
        if self.editing {
            return;
        }
        self.location = self
            .active_webview()
            .and_then(|webview| webview.url())
            .map(|url| url.to_string())
            .unwrap_or_default();
    }

    pub(crate) fn go_back(&mut self) {
        if let Some(w) = self.active_webview().filter(|w| w.can_go_back()) {
            w.go_back(1);
        }
    }
    pub(crate) fn go_forward(&mut self) {
        if let Some(w) = self.active_webview().filter(|w| w.can_go_forward()) {
            w.go_forward(1);
        }
    }
    pub(crate) fn reload(&mut self) {
        let tab = &mut self.tabs[self.active];
        if tab.crashed.take().is_some() {
            if let Some(url) = tab.url() {
                let webview = self.create_webview(url);
                self.tabs[self.active].webview = Some(webview);
                self.activate(self.active);
            }
            return;
        }
        if let Some(w) = &tab.webview {
            w.reload();
        }
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1]);
        let tab = &mut self.tabs[self.active];
        tab.zoom = zoom;
        if let Some(w) = &tab.webview {
            w.set_page_zoom(zoom);
        }
        self.toast(format!("Zoom {}%", (zoom * 100.0).round()));
    }

    pub(crate) fn zoom_step(&mut self, up: bool) {
        let current = self.active_tab().zoom;
        let next = if up {
            ZOOM_STEPS.iter().copied().find(|z| *z > current + 0.001)
        } else {
            ZOOM_STEPS.iter().rev().copied().find(|z| *z < current - 0.001)
        };
        if let Some(z) = next {
            self.set_zoom(z);
        }
    }

    pub(crate) fn toggle_window_fullscreen(&mut self) {
        self.window_fullscreen = !self.window_fullscreen;
        self.window.set_fullscreen(self.window_fullscreen.then_some(Fullscreen::Borderless(None)));
    }

    pub(crate) fn toggle_bookmark(&mut self) {
        let tab = self.active_tab();
        if let Some(url) = tab.url() {
            let title = tab.title();
            self.profile.toggle_bookmark(url.as_str(), &title);
            let msg = if self.profile.is_bookmarked(url.as_str()) { "Bookmarked" } else { "Bookmark removed" };
            self.toast(msg);
        }
    }

    pub(crate) fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
        self.window.request_redraw();
    }

    pub(crate) fn open_private_window(&self) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).arg("--private").spawn();
        }
    }

    pub(crate) fn start_download(&mut self, url: Url, referer: Option<Url>) {
        if !matches!(url.scheme(), "http" | "https") {
            self.toast("Only web (http/https) links can be downloaded");
            return;
        }
        // Send the site's cookies so downloads behind a login work.
        let cookies = self
            .servo
            .site_data_manager()
            .cookies_for_url(url.clone(), CookieSource::HTTP)
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        let target = if self.profile.settings.ask_where_to_save {
            let name = url.path_segments().and_then(|mut s| s.next_back()).unwrap_or("download").to_string();
            match rfd::FileDialog::new().set_file_name(name).save_file() {
                Some(path) => Some(path),
                None => return,
            }
        } else {
            None
        };
        self.downloads.start(url.to_string(), Some(cookies), referer.map(|r| r.to_string()), target);
        self.toast("Download started — Ctrl+J to view");
    }

    pub(crate) fn clear_site_data(&mut self) {
        let manager = self.servo.site_data_manager();
        manager.clear_cookies(None);
        let sites = manager.site_data(StorageType::all());
        let names: Vec<String> = sites.iter().map(|s| s.name()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        manager.clear_site_data(&refs, StorageType::all());
        self.toast("Cookies and site data cleared");
    }

    /// Re-apply the theme to the UI and to pages (`prefers-color-scheme`).
    pub(crate) fn apply_theme(&mut self, ctx: &egui::Context) {
        let theme = match self.profile.settings.theme {
            ThemeChoice::Light => servo::Theme::Light,
            ThemeChoice::Dark => servo::Theme::Dark,
            ThemeChoice::System => match ctx.theme() {
                egui::Theme::Dark => servo::Theme::Dark,
                egui::Theme::Light => servo::Theme::Light,
            },
        };
        if self.page_theme != Some(theme) {
            self.page_theme = Some(theme);
            for webview in self.tabs.iter().filter_map(|t| t.webview.as_ref()) {
                webview.notify_theme_change(theme);
            }
        }
    }

    fn save_session(&mut self) {
        if self.profile.is_private() {
            return;
        }
        let urls: Vec<String> = self.tabs.iter().filter_map(|t| t.url()).map(|u| u.to_string()).collect();
        self.profile.save_session(urls, self.active);
    }

    // ----- event loop -------------------------------------------------------

    /// Let Servo do pending main-thread work and handle what it asked for.
    pub fn pump(&mut self) {
        self.servo.spin_event_loop();

        let events: Vec<Event> = self.delegate.events.borrow_mut().drain(..).collect();
        for event in events {
            match event {
                Event::Opened(webview) => {
                    if let Some(theme) = self.page_theme {
                        webview.notify_theme_change(theme);
                    }
                    self.tabs.insert(self.active + 1, Tab::new(Some(webview), self.profile.settings.default_zoom));
                    self.activate(self.active + 1);
                },
                Event::Closed(id) => {
                    if let Some(i) = self.tab_index(id) {
                        self.close_tab(i);
                    }
                },
                Event::Control(pending) => self.controls.push(pending),
                Event::HideControl(id) => self.controls.retain(|c| c.id != Some(id)),
                Event::FilePicker(picker) => run_file_picker(picker),
                Event::Crashed(id, reason) => {
                    log::error!("Tab crashed: {reason}");
                    if let Some(i) = self.tab_index(id) {
                        self.tabs[i].crashed = Some(reason);
                    }
                },
                Event::Fullscreen(on) => {
                    self.page_fullscreen = on;
                    self.window.set_fullscreen(
                        (on || self.window_fullscreen).then_some(Fullscreen::Borderless(None)),
                    );
                },
                Event::Loaded(webview) => {
                    if let Some(url) = webview.url() {
                        let title = webview.page_title();
                        self.profile.record_visit(&url, title.as_deref());
                    }
                },
                Event::Title(webview) => {
                    if let (Some(url), Some(title)) = (webview.url(), webview.page_title()) {
                        self.profile.update_title(&url, &title);
                    }
                },
                Event::Favicon(id) => {
                    if let Some(i) = self.tab_index(id) {
                        self.tabs[i].favicon_dirty = true;
                    }
                },
                Event::Download(url, referer) => self.start_download(url, referer),
            }
        }
        if self.profile.needs_flush() {
            self.profile.flush(false);
        }
    }

    /// Returns false when the browser should quit.
    pub fn handle_window_event(&mut self, event: WindowEvent) -> bool {
        self.pump();

        match &event {
            WindowEvent::CloseRequested => {
                self.save_session();
                self.profile.flush(true);
                return false;
            },
            WindowEvent::Resized(size) => {
                self.window_context.resize(*size);
                self.window.request_redraw();
            },
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                for webview in self.tabs.iter().filter_map(|t| t.webview.as_ref()) {
                    webview.set_hidpi_scale_factor(Scale::new(*scale_factor as f32));
                }
            },
            WindowEvent::ThemeChanged(_) => self.window.request_redraw(),
            WindowEvent::RedrawRequested => {
                self.redraw();
                if self.wants_quit() {
                    self.save_session();
                    self.profile.flush(true);
                    return false;
                }
                return true;
            },
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            _ => {},
        }

        // Browser shortcuts win over both the interface and the page.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            if key.state == ElementState::Pressed && self.handle_shortcut(key) {
                self.window.request_redraw();
                return true;
            }
        }

        let Some(egui) = self.egui.as_mut() else { return true };
        let response = egui.on_window_event(&self.window, &event);
        if response.repaint {
            self.window.request_redraw();
        }
        let wants_keyboard = egui.egui_ctx.egui_wants_keyboard_input();
        let pointer_on_ui = egui.egui_ctx.egui_is_using_pointer();

        self.forward_to_page(&event, response.consumed || wants_keyboard, pointer_on_ui);
        true
    }

    fn handle_shortcut(&mut self, key: &KeyEvent) -> bool {
        let m = self.modifiers;
        let cmd = if cfg!(target_os = "macos") { m.super_key() } else { m.control_key() };
        let shift = m.shift_key();
        let text = match &key.logical_key {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };

        match (&key.logical_key, text.as_str()) {
            (_, "t") if cmd && shift => self.reopen_closed_tab(),
            (_, "t") if cmd => self.open_home_tab(),
            (_, "n") if cmd && shift => self.open_private_window(),
            (_, "w") if cmd => self.close_tab(self.active),
            (_, "l") if cmd => self.focus_location = true,
            (Key::Named(NamedKey::F6), _) => self.focus_location = true,
            (_, "r") if cmd => self.reload(),
            (Key::Named(NamedKey::F5), _) => self.reload(),
            (_, "d") if cmd => self.toggle_bookmark(),
            (_, "b") if cmd && shift => {
                self.profile.settings.show_bookmarks_bar = !self.profile.settings.show_bookmarks_bar;
                self.profile.mark_dirty();
            },
            (_, "h") if cmd => self.toggle_panel(SidePanel::History),
            (_, "j") if cmd => self.toggle_panel(SidePanel::Downloads),
            (_, "o") if cmd && shift => self.toggle_panel(SidePanel::Bookmarks),
            (_, ",") if cmd => self.toggle_panel(SidePanel::Settings),
            (_, "=" | "+") if cmd => self.zoom_step(true),
            (_, "-") if cmd => self.zoom_step(false),
            (_, "0") if cmd => self.set_zoom(1.0),
            (_, "q") if cmd && cfg!(target_os = "macos") => {
                self.save_session();
                self.profile.flush(true);
                std::process::exit(0);
            },
            (Key::Named(NamedKey::F11), _) => self.toggle_window_fullscreen(),
            (Key::Named(NamedKey::Escape), _) if self.page_fullscreen => {
                if let Some(w) = self.active_webview() {
                    w.exit_fullscreen();
                }
            },
            (Key::Named(NamedKey::Escape), _) if self.menu_open || self.has_popup() => {
                self.menu_open = false;
                self.dismiss_popups();
            },
            (Key::Named(NamedKey::ArrowLeft), _) if m.alt_key() => self.go_back(),
            (Key::Named(NamedKey::ArrowRight), _) if m.alt_key() => self.go_forward(),
            (_, "[") if cmd && cfg!(target_os = "macos") => self.go_back(),
            (_, "]") if cmd && cfg!(target_os = "macos") => self.go_forward(),
            (Key::Named(NamedKey::Tab), _) if m.control_key() => {
                let n = self.tabs.len();
                let next = if shift { (self.active + n - 1) % n } else { (self.active + 1) % n };
                self.activate(next);
            },
            (_, d) if cmd && d.len() == 1 && ("1"..="9").contains(&d) => {
                let n: usize = d.parse().unwrap_or(1);
                let target = if n == 9 { self.tabs.len() - 1 } else { n - 1 };
                if target < self.tabs.len() {
                    self.activate(target);
                }
            },
            _ => return false,
        }
        true
    }

    pub(crate) fn toggle_panel(&mut self, panel: SidePanel) {
        self.panel = if self.panel == panel { SidePanel::None } else { panel };
        self.menu_open = false;
    }

    /// Whether the active tab has a dialog or popup that should capture input.
    pub(crate) fn has_popup(&self) -> bool {
        let Some(id) = self.active_webview().map(|w| w.id()) else { return false };
        self.controls.iter().any(|c| c.webview == id)
    }

    pub(crate) fn dismiss_popups(&mut self) {
        let Some(id) = self.active_webview().map(|w| w.id()) else { return };
        // Dropping a request answers it with "cancel".
        self.controls.retain(|c| {
            c.webview != id
                || matches!(c.control, PageControl::Dialog { .. } | PageControl::Auth { .. } | PageControl::Permission(_))
        });
    }

    fn in_content(&self, (x, y): (f32, f32)) -> bool {
        let r = self.content_rect;
        let p = self.ppp;
        x >= r.left() * p && y >= r.top() * p && x < r.right() * p && y < r.bottom() * p
    }

    fn page_point(&self, (x, y): (f32, f32)) -> DevicePoint {
        DevicePoint::new(x - self.content_rect.left() * self.ppp, y - self.content_rect.top() * self.ppp)
    }

    fn forward_to_page(&mut self, event: &WindowEvent, keyboard_taken: bool, pointer_on_ui: bool) {
        let Some(webview) = self.active_webview().cloned() else { return };
        if self.active_tab().crashed.is_some() {
            return;
        }
        // While a page dialog/popup or the menu is open, the page doesn't get input.
        let blocked = self.has_popup() || self.menu_open;

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let pos = (position.x as f32, position.y as f32);
                let was_inside = self.mouse.is_some_and(|p| self.in_content(p));
                self.mouse = Some(pos);
                if blocked {
                    return;
                }
                if self.in_content(pos) && !pointer_on_ui {
                    webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(self.page_point(pos).into())));
                } else if was_inside {
                    webview.notify_input_event(InputEvent::MouseLeftViewport(MouseLeftViewportEvent::default()));
                }
            },
            WindowEvent::CursorLeft { .. } => {
                if self.mouse.take().is_some_and(|p| self.in_content(p)) {
                    webview.notify_input_event(InputEvent::MouseLeftViewport(MouseLeftViewportEvent::default()));
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if blocked || pointer_on_ui {
                    return;
                }
                let Some(pos) = self.mouse.filter(|p| self.in_content(*p)) else { return };
                let pressed = *state == ElementState::Pressed;
                let button = match button {
                    MouseButton::Left => ServoMouseButton::Primary,
                    MouseButton::Right => ServoMouseButton::Secondary,
                    MouseButton::Middle => ServoMouseButton::Auxiliary,
                    MouseButton::Back => {
                        if pressed {
                            self.go_back();
                        }
                        return;
                    },
                    MouseButton::Forward => {
                        if pressed {
                            self.go_forward();
                        }
                        return;
                    },
                    MouseButton::Other(v) => ServoMouseButton::Other(*v),
                };
                let action = if pressed { MouseButtonAction::Down } else { MouseButtonAction::Up };
                if pressed {
                    // Clicking the page takes keyboard focus away from the interface.
                    if let Some(egui) = &self.egui {
                        egui.egui_ctx.memory_mut(|m| {
                            if let Some(id) = m.focused() {
                                m.surrender_focus(id);
                            }
                        });
                    }
                    self.editing = false;
                    webview.set_focused(true);
                }
                webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                    action,
                    button,
                    self.page_point(pos).into(),
                )));
            },
            WindowEvent::MouseWheel { delta, .. } => {
                if blocked {
                    return;
                }
                let Some(pos) = self.mouse.filter(|p| self.in_content(*p)) else { return };
                let cmd = if cfg!(target_os = "macos") { self.modifiers.super_key() } else { self.modifiers.control_key() };
                if cmd {
                    let up = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y > 0.0,
                        MouseScrollDelta::PixelDelta(d) => d.y > 0.0,
                    };
                    self.zoom_step(up);
                    return;
                }
                let (x, y, mode) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => ((*dx * 76.0) as f64, (*dy * 76.0) as f64, WheelMode::DeltaLine),
                    MouseScrollDelta::PixelDelta(d) => (d.x, d.y, WheelMode::DeltaPixel),
                };
                webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                    WheelDelta { x, y, z: 0.0, mode },
                    self.page_point(pos).into(),
                )));
            },
            WindowEvent::KeyboardInput { event: key, .. } => {
                if blocked || keyboard_taken {
                    return;
                }
                webview.notify_input_event(InputEvent::Keyboard(keyboard_event(key, self.modifiers)));
            },
            _ => {},
        }
    }

    // ----- drawing ----------------------------------------------------------

    fn redraw(&mut self) {
        self.pump();
        self.sync_location();
        self.page_context.make_current().expect("make GL context current");

        let Some(mut egui) = self.egui.take() else { return };
        let window = self.window.clone();
        egui.run(&window, |ui| self.draw(ui));

        let title = match self.active_webview() {
            Some(_) => format!("{} — {APP_NAME}", self.active_tab().title()),
            None => APP_NAME.to_string(),
        };
        let title = if self.profile.is_private() { format!("{title} (Private)") } else { title };
        self.window.set_title(&title);

        if egui.egui_ctx.has_requested_repaint() {
            self.window.request_redraw();
        }

        self.page_context.make_current().expect("make GL context current");
        self.window_context.prepare_for_rendering();
        egui.paint(&self.window);
        self.window_context.present();
        self.egui = Some(egui);
    }

    pub(crate) fn mouse_physical(&self) -> Option<(f32, f32)> {
        self.mouse
    }

    pub(crate) fn egui_ctx(&self) -> Option<egui::Context> {
        self.egui.as_ref().map(|e| e.egui_ctx.clone())
    }

    /// Convert a webview favicon into an egui texture (once per change).
    pub(crate) fn refresh_favicon(&mut self, ctx: &egui::Context, index: usize) {
        let tab = &mut self.tabs[index];
        if !tab.favicon_dirty {
            return;
        }
        tab.favicon_dirty = false;
        let Some(webview) = &tab.webview else { return };
        let Some(image) = webview.favicon() else {
            tab.favicon = None;
            return;
        };
        let (w, h) = (image.width as usize, image.height as usize);
        let data = image.data();
        let rgba: Vec<u8> = match image.format {
            PixelFormat::RGBA8 => data.to_vec(),
            PixelFormat::BGRA8 => data.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect(),
            PixelFormat::RGB8 => data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            PixelFormat::K8 => data.iter().flat_map(|&k| [k, k, k, 255]).collect(),
            PixelFormat::KA8 => data.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        };
        if w == 0 || h == 0 || rgba.len() < w * h * 4 {
            return;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba[..w * h * 4]);
        tab.favicon = Some(ctx.load_texture(format!("favicon-{index}-{w}x{h}"), image, egui::TextureOptions::LINEAR));
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.save_session();
        self.profile.flush(true);
        self.controls.clear();
        self.tabs.clear();
        if let Some(mut egui) = self.egui.take() {
            egui.destroy();
        }
    }
}

fn run_file_picker(mut picker: FilePicker) {
    let mut dialog = rfd::FileDialog::new();
    let extensions: Vec<String> = picker
        .filter_patterns()
        .iter()
        .map(|p| p.0.trim_start_matches('.').to_string())
        .filter(|e| !e.is_empty() && !e.contains('/') && !e.contains('*'))
        .collect();
    if !extensions.is_empty() {
        dialog = dialog.add_filter("Allowed files", &extensions);
    }
    let paths = if picker.allow_select_multiple() {
        dialog.pick_files().unwrap_or_default()
    } else {
        dialog.pick_file().into_iter().collect()
    };
    if paths.is_empty() {
        picker.dismiss();
    } else {
        picker.select(&paths);
        picker.submit();
    }
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The browser window: a native egui interface, with each tab's page shown
//! by the system's web engine (through wry) in a native view placed over the
//! page area. Popups that float over the page are separate small windows
//! (overlay.rs), because nothing drawn in the main window can cover the
//! page's native view.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use egui_glow::EguiGlow;
use url::Url;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{NewWindowResponse, PageLoadEvent, WebContext, WebView, WebViewBuilder};

use crate::WakeUp;
use crate::blocker::{Blocker, base_domain};
use crate::devtools::Source;
use crate::downloads::{Downloads, looks_like_download};
use crate::find::FindState;
use crate::gl::GlWindow;
use crate::location::input_to_url;
use crate::overlay::{OverlayKind, Overlays};
use crate::profile::{NewTabPage, Profile, SavedTab, SearchEngine};
use crate::reader::ReaderDoc;
use crate::updates::UpdateStatus;

pub const APP_NAME: &str = "Ferrum";
const ZOOM_STEPS: [f32; 13] = [0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
const SLEEP_CHECK: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Engine -> browser messages
// ---------------------------------------------------------------------------

/// Something a page's engine reported. The engine's callbacks only queue
/// these; `Browser::pump` handles them.
enum Event {
    Title(u64, String),
    Load(u64, PageLoadEvent, String),
    NewTab(String),
    Download(u64, String),
    Ipc(u64, String),
}

#[derive(Clone)]
struct Queue {
    events: Rc<RefCell<Vec<Event>>>,
    window: Rc<Window>,
}

impl Queue {
    fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
        self.window.request_redraw();
    }
}

/// A keyboard shortcut, from winit or from a page (via the page script).
pub struct Shortcut {
    /// `"t"`, `"F5"`, `"ArrowLeft"`, `"Tab"`, `"Escape"`…
    pub key: String,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub alt: bool,
}

// ---------------------------------------------------------------------------
// Browser state
// ---------------------------------------------------------------------------

/// Pages drawn by Ferrum itself rather than loaded by the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InternalPage {
    Settings,
    Shortcuts,
    /// A page's HTML source (the text is in `Tab::source`).
    Source,
}

impl InternalPage {
    pub const SETTINGS_URL: &'static str = "ferrum://settings";
    pub const SHORTCUTS_URL: &'static str = "ferrum://shortcuts";

    pub fn from_url(url: &Url) -> Option<Self> {
        match (url.scheme(), url.host_str().unwrap_or(""), url.path()) {
            ("ferrum", "settings", _) => Some(InternalPage::Settings),
            ("ferrum", "shortcuts", _) => Some(InternalPage::Shortcuts),
            ("about", _, "settings" | "preferences") => Some(InternalPage::Settings),
            ("about", _, "shortcuts" | "keyboard") => Some(InternalPage::Shortcuts),
            _ => None,
        }
    }

    pub fn url(self) -> Url {
        match self {
            InternalPage::Settings => Url::parse(Self::SETTINGS_URL).expect("valid URL"),
            InternalPage::Shortcuts => Url::parse(Self::SHORTCUTS_URL).expect("valid URL"),
            InternalPage::Source => Url::parse("view-source:about:blank").expect("valid URL"),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            InternalPage::Settings => "Settings",
            InternalPage::Shortcuts => "Keyboard shortcuts",
            InternalPage::Source => "Page source",
        }
    }
}

/// A tab is a page in the engine, a page drawn by Ferrum, or the start page.
pub struct Tab {
    pub id: u64,
    pub webview: Option<WebView>,
    pub title: String,
    pub page_url: Option<Url>,
    pub is_loading: bool,
    pub zoom: f32,
    pub favicon: Option<egui::TextureHandle>,
    pub upgraded_to_https: bool,
    /// A restored or sleeping tab: its page loads when the tab is selected.
    pub pending: Option<(Url, String)>,
    pub internal: Option<InternalPage>,
    pub pinned: bool,
    pub muted: bool,
    pub playing: bool,
    pub reader: Option<ReaderDoc>,
    pub source: Option<Source>,
    pub last_used: Instant,
    pub search: Option<(SearchEngine, String)>,
    /// Ad and tracker requests blocked on the current page (counted by the page script).
    pub blocked: u32,
    /// Whether the page's view is currently shown, and where.
    shown: bool,
    bounds: (i32, i32, u32, u32),
}

impl Tab {
    fn new(id: u64, zoom: f32) -> Self {
        Self {
            id,
            webview: None,
            title: String::new(),
            page_url: None,
            is_loading: false,
            zoom,
            favicon: None,
            upgraded_to_https: false,
            pending: None,
            internal: None,
            pinned: false,
            muted: false,
            playing: false,
            reader: None,
            source: None,
            last_used: Instant::now(),
            search: None,
            blocked: 0,
            // New pages start visible (WebKitGTK only lays out a page that was shown);
            // layout_pages hides the ones that shouldn't be.
            shown: true,
            bounds: (0, 0, 0, 0),
        }
    }

    pub fn title(&self) -> String {
        if let Some(source) = &self.source {
            return format!("Source: {}", source.url.trim_start_matches("https://").trim_start_matches("http://"));
        }
        if let Some(page) = self.internal {
            return page.title().to_string();
        }
        if let Some((url, title)) = &self.pending {
            return if title.trim().is_empty() { url.host_str().unwrap_or(url.as_str()).to_string() } else { title.clone() };
        }
        if self.webview.is_none() {
            return "New Tab".to_string();
        }
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        self.page_url.as_ref().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_else(|| "Loading…".into())
    }

    pub fn url(&self) -> Option<Url> {
        if let Some(source) = &self.source {
            return Url::parse(&format!("view-source:{}", source.url)).ok();
        }
        if let Some(page) = self.internal {
            return Some(page.url());
        }
        if let Some((url, _)) = &self.pending {
            return Some(url.clone());
        }
        self.webview.as_ref().and(self.page_url.clone())
    }

    pub fn loading(&self) -> bool {
        self.webview.is_some() && self.is_loading
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidePanel {
    None,
    History,
    Bookmarks,
    Downloads,
}

/// A site asking to show notifications: (tab id, host).
pub type PermissionAsk = (u64, String);

pub struct Browser {
    pub(crate) window: Rc<Window>,
    pub(crate) gl_window: Rc<GlWindow>,
    pub(crate) gl: Arc<glow::Context>,
    egui: Option<EguiGlow>,
    web_context: WebContext,
    events: Rc<RefCell<Vec<Event>>>,

    pub(crate) profile: Profile,
    pub(crate) downloads: Downloads,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    next_tab_id: u64,
    closed_tabs: Vec<String>,

    // Address bar
    pub(crate) location: String,
    pub(crate) editing: bool,
    pub(crate) focus_location: bool,
    /// Take keyboard focus away from the address bar (after navigating).
    pub(crate) blur_location: bool,
    pub(crate) suggestion_index: Option<usize>,
    pub(crate) start_query: String,

    // Interface state
    pub(crate) panel: SidePanel,
    pub(crate) menu_open: bool,
    pub(crate) history_filter: String,
    pub(crate) window_fullscreen: bool,
    pub(crate) toast: Option<(String, Instant)>,
    pub(crate) style_key: u64,
    pub(crate) start_image: Option<(String, Option<egui::TextureHandle>)>,
    pub(crate) find: FindState,
    repaint_at: Arc<Mutex<Option<Instant>>>,
    pub(crate) last_title: String,
    pub(crate) settings_section: crate::settings_page::Section,
    pub(crate) settings_query: String,
    pub(crate) blocker: Rc<Blocker>,
    pub(crate) update: Option<Arc<Mutex<UpdateStatus>>>,
    pub(crate) site_info_open: bool,
    /// The tab whose right-click menu is open, and where.
    pub(crate) tab_menu: Option<(usize, egui::Pos2)>,
    /// A web notification: (site, title, body, since).
    pub(crate) notification: Option<(String, String, String, Instant)>,
    /// Sites waiting for an answer to "show notifications?".
    pub(crate) permission_asks: Vec<PermissionAsk>,
    pub(crate) remember_permission: bool,
    pub(crate) search_blocked: Option<(u64, SearchEngine, String)>,
    pub(crate) overlays: Overlays,
    /// Popups requested while drawing this frame, shown in overlay windows.
    pub(crate) overlay_requests: Vec<(OverlayKind, egui::Rect)>,
    /// The page script, and the settings it was built for.
    page_script: (u64, String),
    pub(crate) proxy: EventLoopProxy<WakeUp>,
    favicons: (mpsc::Sender<(u64, egui::ColorImage)>, mpsc::Receiver<(u64, egui::ColorImage)>),
    last_sleep_check: Instant,
    last_url_poll: Instant,
    pending_reader: Option<(u64, Arc<Mutex<Option<Option<ReaderDoc>>>>)>,
    pending_source: Option<Arc<Mutex<Option<(String, String)>>>>,
    search_check: Arc<Mutex<Option<(u64, SearchEngine, String)>>>,
    /// Whether the interface took the keyboard back from the page.
    page_unfocused: bool,
    pub(crate) custom_title_bar_at_start: bool,
    on_edge: Option<winit::window::ResizeDirection>,
    pub(crate) quit_requested: bool,

    // Page geometry (egui points) and input
    pub(crate) content_rect: egui::Rect,
    pub(crate) ppp: f32,
    pub(crate) modifiers: ModifiersState,
}

impl Browser {
    pub fn new(event_loop: &ActiveEventLoop, proxy: EventLoopProxy<WakeUp>, initial_url: Option<String>, private: bool) -> Self {
        let mut profile = Profile::load(if private { None } else { Profile::default_dir() });
        profile.settings.fit_search_engine_to_region();
        crate::timing("profile loaded");

        let title = if private { format!("{APP_NAME} (Private)") } else { APP_NAME.to_string() };
        let custom_title_bar = profile.settings.custom_title_bar;
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 840.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(480.0, 320.0));
        // Like Chrome, put the tabs in the title bar and draw our own window buttons.
        // On macOS the system's traffic lights stay, over a transparent title bar.
        #[cfg(target_os = "macos")]
        let attributes = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes
                .with_titlebar_transparent(custom_title_bar)
                .with_title_hidden(custom_title_bar)
                .with_fullsize_content_view(custom_title_bar)
        };
        #[cfg(not(target_os = "macos"))]
        let attributes = attributes.with_decorations(!custom_title_bar);

        let (gl_window, gl) = GlWindow::new(event_loop, attributes);
        let gl_window = Rc::new(gl_window);
        let gl = Arc::new(gl);
        let window = gl_window.window.clone();
        crate::timing("window created");
        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon-256.png")) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                window.set_window_icon(winit::window::Icon::from_rgba(rgba.into_raw(), w, h).ok());
            }
        }

        let egui = EguiGlow::new(event_loop, gl.clone(), None, None, false);
        crate::ui::style(&egui.egui_ctx, &profile.settings);
        let repaint_at: Arc<Mutex<Option<Instant>>> = Arc::default();
        {
            let repaint_at = repaint_at.clone();
            let proxy = proxy.clone();
            egui.egui_ctx.set_request_repaint_callback(move |info| {
                let at = Instant::now() + info.delay;
                if let Ok(mut slot) = repaint_at.lock() {
                    if slot.is_none_or(|current| at < current) {
                        *slot = Some(at);
                    }
                }
                if info.delay.is_zero() {
                    let _ = proxy.send_event(WakeUp);
                }
            });
        }
        crate::timing("interface ready");

        let blocker = Rc::new(Blocker::new(profile.settings.block_trackers, &profile.settings.blocker_exceptions));
        blocker.load_list(Profile::default_dir());
        // Engine-level blocking: content rules (macOS, Linux) or a request filter (Windows).
        crate::adblock::prepare(&blocker.domains(), &profile.settings.blocker_exceptions, profile.settings.block_trackers);

        let notify = proxy.clone();
        let downloads = Downloads::new(Arc::new(move || {
            let _ = notify.send_event(WakeUp);
        }));
        let web_context = WebContext::new(profile.engine_dir());

        let mut browser = Self {
            window: window.clone(),
            gl_window,
            gl,
            egui: Some(egui),
            web_context,
            events: Rc::default(),
            profile,
            downloads,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            closed_tabs: Vec::new(),
            location: String::new(),
            editing: false,
            focus_location: false,
            blur_location: false,
            suggestion_index: None,
            start_query: String::new(),
            panel: SidePanel::None,
            menu_open: false,
            history_filter: String::new(),
            window_fullscreen: false,
            toast: None,
            style_key: 0,
            start_image: None,
            find: FindState::default(),
            repaint_at,
            last_title: String::new(),
            settings_section: Default::default(),
            settings_query: String::new(),
            blocker,
            update: None,
            site_info_open: false,
            tab_menu: None,
            notification: None,
            permission_asks: Vec::new(),
            remember_permission: true,
            search_blocked: None,
            overlays: Overlays::default(),
            overlay_requests: Vec::new(),
            page_script: (0, String::new()),
            proxy,
            favicons: mpsc::channel(),
            last_sleep_check: Instant::now(),
            last_url_poll: Instant::now(),
            pending_reader: None,
            pending_source: None,
            search_check: Arc::default(),
            page_unfocused: false,
            custom_title_bar_at_start: custom_title_bar,
            on_edge: None,
            quit_requested: false,
            content_rect: egui::Rect::NOTHING,
            ppp: window.scale_factor() as f32,
            modifiers: ModifiersState::empty(),
        };

        if let Some(url) = initial_url.as_deref().and_then(|u| input_to_url(u, &browser.profile.settings)) {
            browser.open_tab(Some(url));
        } else if browser.profile.settings.restore_session && !browser.profile.is_private() {
            // Restored tabs stay unloaded until selected; only the active one loads now.
            let (saved, active) = browser.profile.load_session();
            let zoom = browser.profile.settings.default_zoom;
            for saved in saved {
                if let Ok(url) = Url::parse(&saved.url) {
                    let id = browser.new_id();
                    let mut tab = Tab::new(id, zoom);
                    tab.pending = Some((url, saved.title));
                    tab.pinned = saved.pinned;
                    browser.tabs.push(tab);
                }
            }
            if !browser.tabs.is_empty() {
                browser.activate(active);
            }
        }
        if browser.tabs.is_empty() {
            browser.open_home_tab();
        }
        browser.downloads.dir = browser.download_dir();
        crate::timing("tabs opened");
        browser.maybe_check_for_updates();
        browser
    }

    fn new_id(&mut self) -> u64 {
        self.next_tab_id += 1;
        self.next_tab_id
    }

    // ----- pages ------------------------------------------------------------

    /// The script added to every page; rebuilt when the settings it uses change.
    fn page_script(&mut self) -> String {
        let s = &self.profile.settings;
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (s.block_trackers, &s.blocker_exceptions, self.blocker.domain_count(), &s.site_permissions, s.allow_notifications).hash(&mut h);
            h.finish()
        };
        if self.page_script.0 != key || self.page_script.1.is_empty() {
            let notifications: Vec<(String, bool)> = s
                .site_permissions
                .iter()
                .filter_map(|(host, f)| f.get("notifications").map(|ok| (host.clone(), *ok)))
                .collect();
            let script = crate::pagescript::script(
                s.block_trackers,
                &self.blocker.domains(),
                &s.blocker_exceptions,
                &notifications,
                s.allow_notifications,
            );
            self.page_script = (key, script);
        }
        self.page_script.1.clone()
    }

    /// Create the engine's view for a tab.
    pub(crate) fn create_webview(&mut self, tab_id: u64, url: &Url) -> Option<WebView> {
        let script = self.page_script();
        let queue = Queue { events: self.events.clone(), window: self.window.clone() };
        let https_only = self.profile.settings.https_only;
        let user_agent = self.profile.settings.user_agent.trim().to_string();
        let private = self.profile.is_private();
        let (q1, q2, q3, q4, q5) = (queue.clone(), queue.clone(), queue.clone(), queue.clone(), queue.clone());
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_url(url.as_str())
            .with_initialization_script(script)
            .with_devtools(true)
            .with_visible(true)
            .with_bounds(wry::Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(1, 1).into() })
            .with_clipboard(true)
            .with_hotkeys_zoom(true)
            .with_back_forward_navigation_gestures(true)
            .with_autoplay(true)
            .with_incognito(private)
            .with_ipc_handler(move |request| q1.push(Event::Ipc(tab_id, request.into_body())))
            .with_document_title_changed_handler(move |title| q2.push(Event::Title(tab_id, title)))
            .with_on_page_load_handler(move |event, url| q3.push(Event::Load(tab_id, event, url)))
            .with_new_window_req_handler(move |url, _| {
                q4.push(Event::NewTab(url));
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |url, _path| {
                // Ferrum's own downloader shows progress and uses the site's cookies.
                q5.push(Event::Download(tab_id, url));
                false
            })
            .with_navigation_handler(move |url| {
                if let Ok(parsed) = Url::parse(&url) {
                    if matches!(parsed.scheme(), "http" | "https") && looks_like_download(&parsed) {
                        queue.push(Event::Download(tab_id, url));
                        return false;
                    }
                    if https_only && parsed.scheme() == "http" {
                        queue.push(Event::Load(tab_id, PageLoadEvent::Started, url.replacen("http://", "https://", 1)));
                        return false;
                    }
                }
                true
            });
        if !user_agent.is_empty() {
            builder = builder.with_user_agent(user_agent);
        }
        match builder.build_as_child(&*self.window) {
            Ok(webview) => {
                crate::adblock::attach(&webview);
                let zoom = self.profile.settings.default_zoom;
                if (zoom - 1.0).abs() > f32::EPSILON {
                    let _ = webview.zoom(zoom as f64);
                }
                Some(webview)
            },
            Err(e) => {
                log::error!("Could not create a web view: {e}");
                self.toast(format!("Could not open the page: {e}"));
                None
            },
        }
    }

    fn home_url(&self) -> Option<Url> {
        let home = self.profile.settings.home_page.trim();
        if home.is_empty() { None } else { input_to_url(home, &self.profile.settings) }
    }

    pub(crate) fn open_home_tab(&mut self) {
        let home = self.home_url();
        self.open_tab(home);
        if self.tabs[self.active].webview.is_none() {
            self.focus_location = true;
        }
    }

    pub(crate) fn new_tab(&mut self) {
        let url = match self.profile.settings.new_tab_page {
            NewTabPage::StartPage => None,
            NewTabPage::HomePage => self.home_url(),
            NewTabPage::Blank => Url::parse("about:blank").ok(),
        };
        self.open_tab(url);
        self.focus_location = true;
    }

    pub(crate) fn open_tab(&mut self, url: Option<Url>) {
        let id = self.new_id();
        let mut tab = Tab::new(id, self.profile.settings.default_zoom);
        if let Some(url) = url {
            if let Some(page) = InternalPage::from_url(&url) {
                tab.internal = Some(page);
            } else {
                tab.webview = self.create_webview(id, &url);
                tab.page_url = Some(url);
                tab.is_loading = true;
            }
        }
        self.tabs.push(tab);
        self.activate(self.tabs.len() - 1);
    }

    /// Opened in the background: it loads when selected, so it costs nothing until then.
    pub(crate) fn open_background_tab(&mut self, url: Url) {
        let id = self.new_id();
        let mut tab = Tab::new(id, self.profile.settings.default_zoom);
        tab.pending = Some((url, String::new()));
        let at = (self.active + 1).min(self.tabs.len());
        self.tabs.insert(at, tab);
        self.toast("Opened in a new tab");
    }

    pub(crate) fn activate(&mut self, index: usize) {
        self.active = index.min(self.tabs.len().saturating_sub(1));
        if let Some((url, title)) = self.tabs.get_mut(self.active).and_then(|t| t.pending.take()) {
            if let Some(page) = InternalPage::from_url(&url) {
                self.tabs[self.active].internal = Some(page);
            } else {
                let id = self.tabs[self.active].id;
                let zoom = self.tabs[self.active].zoom;
                let webview = self.create_webview(id, &url);
                if let Some(w) = &webview {
                    let _ = w.zoom(zoom as f64);
                }
                let tab = &mut self.tabs[self.active];
                tab.webview = webview;
                tab.page_url = Some(url);
                tab.title = title;
                tab.is_loading = true;
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.last_used = Instant::now();
        }
        self.site_info_open = false;
        self.tab_menu = None;
        self.editing = false;
        self.suggestion_index = None;
        self.find.reset();
        self.sync_location();
        self.profile.mark_dirty();
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
        drop(tab);
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

    /// Remove a tab without re-activating (used when closing several at once).
    pub(crate) fn close_tab_quietly(&mut self, index: usize) {
        if index >= self.tabs.len() || self.tabs.len() == 1 {
            return;
        }
        let tab = self.tabs.remove(index);
        if let Some(url) = tab.url() {
            self.closed_tabs.push(url.to_string());
        }
        if self.active >= self.tabs.len() || index < self.active {
            self.active = self.active.saturating_sub(1);
        }
    }

    pub(crate) fn duplicate_tab(&mut self, index: usize) {
        if let Some(url) = self.tabs.get(index).and_then(|t| t.url()) {
            let zoom = self.tabs[index].zoom;
            let title = self.tabs[index].title();
            let id = self.new_id();
            let mut tab = Tab::new(id, zoom);
            tab.pending = Some((url, title));
            self.tabs.insert(index + 1, tab);
            self.activate(index + 1);
        }
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

    fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub(crate) fn navigate_input(&mut self, input: &str) {
        if let Some(url) = input_to_url(input, &self.profile.settings) {
            let is_search = url == self.profile.settings.search_url(input.trim());
            let upgraded = !is_search && url.scheme() == "https" && !input.trim().to_ascii_lowercase().starts_with("https://");
            self.navigate(url);
            let engine = self.profile.settings.search_engine;
            let tab = &mut self.tabs[self.active];
            tab.upgraded_to_https = upgraded;
            tab.search = is_search.then(|| (engine, input.trim().to_string()));
        }
    }

    pub(crate) fn navigate(&mut self, url: Url) {
        self.blur_location = true;
        self.find.reset();
        self.editing = false;
        self.suggestion_index = None;
        if let Some(page) = InternalPage::from_url(&url) {
            let tab = &mut self.tabs[self.active];
            tab.webview = None;
            tab.favicon = None;
            tab.source = None;
            tab.reader = None;
            tab.shown = false;
            tab.internal = Some(page);
            self.activate(self.active);
            return;
        }
        let tab = &mut self.tabs[self.active];
        tab.internal = None;
        tab.source = None;
        tab.reader = None;
        tab.upgraded_to_https = false;
        tab.page_url = Some(url.clone());
        tab.is_loading = true;
        match &tab.webview {
            Some(webview) => {
                let _ = webview.load_url(url.as_str());
            },
            None => {
                let id = tab.id;
                let webview = self.create_webview(id, &url);
                self.tabs[self.active].webview = webview;
            },
        }
        self.sync_location();
    }

    pub(crate) fn go_home(&mut self) {
        match self.home_url() {
            Some(url) => self.navigate(url),
            None => {
                let id = self.new_id();
                self.tabs[self.active] = Tab::new(id, self.profile.settings.default_zoom);
                self.activate(self.active);
                self.focus_location = true;
            },
        }
    }

    pub(crate) fn sync_location(&mut self) {
        if self.editing {
            return;
        }
        self.location = self.tabs.get(self.active).and_then(|t| t.url()).map(|url| url.to_string()).unwrap_or_default();
    }

    pub(crate) fn can_go_back(&self) -> bool {
        self.active_webview().is_some_and(|w| w.can_go_back().unwrap_or(false))
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        self.active_webview().is_some_and(|w| w.can_go_forward().unwrap_or(false))
    }

    pub(crate) fn go_back(&mut self) {
        if let Some(w) = self.active_webview() {
            let _ = w.go_back();
        }
    }

    pub(crate) fn go_forward(&mut self) {
        if let Some(w) = self.active_webview() {
            let _ = w.go_forward();
        }
    }

    pub(crate) fn reload(&mut self) {
        if let Some(w) = self.active_webview() {
            let _ = w.reload();
        }
    }

    pub(crate) fn stop(&mut self) {
        if let Some(w) = self.active_webview() {
            let _ = w.evaluate_script("window.stop()");
        }
        self.tabs[self.active].is_loading = false;
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1]);
        let tab = &mut self.tabs[self.active];
        tab.zoom = zoom;
        if let Some(w) = &tab.webview {
            let _ = w.zoom(zoom as f64);
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
            .active_webview()
            .and_then(|w| w.cookies_for_url(url.as_str()).ok())
            .unwrap_or_default()
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
        let any = self.tabs.iter().find_map(|t| t.webview.as_ref());
        match any {
            Some(w) => {
                let _ = w.clear_all_browsing_data();
                self.toast("Cookies and site data cleared");
            },
            None => self.toast("Open a page first, then clear site data"),
        }
    }

    fn save_session(&mut self) {
        if self.profile.is_private() {
            return;
        }
        let mut tabs: Vec<SavedTab> = Vec::new();
        let mut active = 0;
        for (i, t) in self.tabs.iter().enumerate() {
            if i == self.active {
                active = tabs.len();
            }
            if t.source.is_some() {
                continue;
            }
            if let Some(u) = t.url() {
                tabs.push(SavedTab { url: u.to_string(), title: t.title(), pinned: t.pinned });
            }
        }
        let active = active.min(tabs.len().saturating_sub(1));
        self.profile.save_session(tabs, active);
    }

    // ----- event loop -------------------------------------------------------

    /// Handle what the engine and background threads reported.
    pub fn pump(&mut self) {
        let events: Vec<Event> = std::mem::take(&mut *self.events.borrow_mut());
        for event in events {
            match event {
                Event::Title(id, title) => {
                    if let Some(i) = self.tab_index(id) {
                        self.tabs[i].title = title.clone();
                        if let Some(url) = self.tabs[i].webview.as_ref().and_then(|w| w.url().ok()).and_then(|u| Url::parse(&u).ok()) {
                            self.profile.update_title(&url, &title);
                            self.tabs[i].page_url = Some(url);
                        }
                    }
                },
                Event::Load(id, event, url) => {
                    let Some(i) = self.tab_index(id) else { continue };
                    let parsed = Url::parse(&url).ok();
                    match event {
                        PageLoadEvent::Started => {
                            let tab = &mut self.tabs[i];
                            // HTTPS-only: an http:// navigation was stopped; load the https:// address.
                            if self.profile.settings.https_only && url.starts_with("https://") && tab.page_url.as_ref().map(|u| u.as_str()) != Some(url.as_str()) {
                                if let Some(w) = &tab.webview {
                                    let _ = w.load_url(&url);
                                }
                            }
                            tab.is_loading = true;
                            tab.reader = None;
                            tab.playing = false;
                            tab.blocked = 0;
                            if parsed.is_some() {
                                tab.page_url = parsed;
                            }
                            if i == self.active {
                                self.find.reset();
                            }
                        },
                        PageLoadEvent::Finished => {
                            let tab = &mut self.tabs[i];
                            tab.is_loading = false;
                            if parsed.is_some() {
                                tab.page_url = parsed.clone();
                            }
                            if tab.muted {
                                if let Some(w) = &tab.webview {
                                    let _ = w.evaluate_script(&crate::pagescript::mute(true));
                                }
                            }
                            if let Some(url) = parsed {
                                let title = (!tab.title.is_empty()).then(|| tab.title.clone());
                                self.profile.record_visit(&url, title.as_deref());
                            }
                            self.check_search_blocked(id);
                        },
                    }
                    self.sync_location();
                },
                Event::NewTab(url) => {
                    if let Ok(url) = Url::parse(&url) {
                        let id = self.new_id();
                        let mut tab = Tab::new(id, self.profile.settings.default_zoom);
                        tab.webview = self.create_webview(id, &url);
                        tab.page_url = Some(url);
                        tab.is_loading = true;
                        let at = (self.active + 1).min(self.tabs.len());
                        self.tabs.insert(at, tab);
                        self.activate(at);
                    }
                },
                Event::Download(id, url) => {
                    if let Ok(url) = Url::parse(&url) {
                        let referer = self.tab_index(id).and_then(|i| self.tabs[i].page_url.clone());
                        self.start_download(url, referer);
                    }
                },
                Event::Ipc(id, message) => self.handle_page_message(id, &message),
            }
        }
        let found = self.search_check.lock().ok().and_then(|mut s| s.take());
        if let Some(found) = found {
            self.search_blocked = Some(found);
            self.window.request_redraw();
        }
        while let Ok((id, image)) = self.favicons.1.try_recv() {
            if let (Some(i), Some(ctx)) = (self.tab_index(id), self.egui_ctx()) {
                self.tabs[i].favicon = Some(ctx.load_texture(format!("favicon-{id}"), image, egui::TextureOptions::LINEAR));
                self.window.request_redraw();
            }
        }
        self.finish_async_work();
        self.sleep_idle_tabs();
        self.save_if_due();
    }

    /// A message from Ferrum's page script.
    fn handle_page_message(&mut self, id: u64, message: &str) {
        let Ok(m) = serde_json::from_str::<serde_json::Value>(message) else { return };
        let Some(i) = self.tab_index(id) else { return };
        match m["t"].as_str().unwrap_or("") {
            "key" => {
                let shortcut = Shortcut {
                    key: m["key"].as_str().unwrap_or("").to_string(),
                    ctrl: m["ctrl"].as_bool().unwrap_or(false),
                    meta: m["meta"].as_bool().unwrap_or(false),
                    shift: m["shift"].as_bool().unwrap_or(false),
                    alt: m["alt"].as_bool().unwrap_or(false),
                };
                if i == self.active && self.handle_shortcut(&shortcut) {
                    self.window.request_redraw();
                }
            },
            "click" => {
                // A click on the page closes Ferrum's popups.
                if i == self.active {
                    self.close_popups();
                }
            },
            "media" => {
                self.tabs[i].playing = m["playing"].as_bool().unwrap_or(false);
                self.window.request_redraw();
            },
            "blocked" => {
                self.tabs[i].blocked = m["count"].as_u64().unwrap_or(0) as u32;
                self.window.request_redraw();
            },
            "icon" => {
                if let Some(href) = m["href"].as_str() {
                    self.fetch_favicon(id, href.to_string());
                }
            },
            "notify" => {
                let site = self.tabs[i].page_url.as_ref().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
                let title = m["title"].as_str().unwrap_or("").to_string();
                let body = m["body"].as_str().unwrap_or("").to_string();
                crate::notify::system_notification(&site, &title, &body, self.window.has_focus());
                self.notification = Some((site, title, body, Instant::now()));
                self.window.request_redraw();
            },
            "permission" => {
                let host = self.tabs[i].page_url.as_ref().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
                let remembered = self.profile.settings.site_permissions.get(&host).and_then(|f| f.get("notifications")).copied();
                match remembered {
                    Some(allowed) => self.answer_notifications(id, allowed),
                    None if !self.profile.settings.allow_notifications => self.answer_notifications(id, false),
                    None => {
                        if !self.permission_asks.iter().any(|(t, _)| *t == id) {
                            self.permission_asks.push((id, host));
                        }
                        self.window.request_redraw();
                    },
                }
            },
            _ => {},
        }
    }

    /// Tell a page whether it may show notifications.
    pub(crate) fn answer_notifications(&mut self, tab_id: u64, allowed: bool) {
        if let Some(w) = self.tab_index(tab_id).and_then(|i| self.tabs[i].webview.as_ref()) {
            let state = if allowed { "granted" } else { "denied" };
            let _ = w.evaluate_script(&format!("window.__ferrumPermission && window.__ferrumPermission({state:?})"));
        }
    }

    /// Answer the oldest "show notifications?" question.
    pub(crate) fn answer_permission_ask(&mut self, allowed: bool) {
        if self.permission_asks.is_empty() {
            return;
        }
        let (tab_id, host) = self.permission_asks.remove(0);
        if self.remember_permission && !host.is_empty() {
            self.profile.settings.site_permissions.entry(host).or_default().insert("notifications".into(), allowed);
            self.profile.mark_dirty();
        }
        self.answer_notifications(tab_id, allowed);
    }

    fn fetch_favicon(&self, id: u64, href: String) {
        let sender = self.favicons.0.clone();
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("favicon".into()).spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
            let Ok(mut response) = agent.get(&href).call() else { return };
            let Ok(bytes) = response.body_mut().with_config().limit(1024 * 1024).read_to_vec() else { return };
            let Ok(img) = image::load_from_memory(&bytes) else { return };
            let img = img.thumbnail(32, 32).to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let _ = sender.send((id, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())));
            let _ = proxy.send_event(WakeUp);
        });
    }

    pub(crate) fn print(&mut self) {
        self.menu_open = false;
        match self.active_webview() {
            Some(w) => {
                let _ = w.print();
            },
            None => self.toast("Open a page to print it"),
        }
    }

    pub(crate) fn open_find(&mut self) {
        if self.active_webview().is_some() {
            self.find.open = true;
            self.find.focus = true;
            self.menu_open = false;
        }
    }

    pub(crate) fn find_step(&mut self, delta: i32) {
        if let Some(webview) = self.tabs.get(self.active).and_then(|t| t.webview.as_ref()) {
            self.find.step(webview, &self.proxy, delta);
        }
    }

    pub(crate) fn close_find(&mut self) {
        let webview = self.tabs.get(self.active).and_then(|t| t.webview.as_ref());
        self.find.close(webview, &self.proxy);
    }

    /// Close the menu and every popup.
    pub(crate) fn close_popups(&mut self) {
        let any = self.menu_open || self.site_info_open || self.tab_menu.is_some() || self.editing;
        self.menu_open = false;
        self.site_info_open = false;
        self.tab_menu = None;
        if self.editing {
            self.editing = false;
            self.sync_location();
        }
        if any {
            self.window.request_redraw();
        }
    }

    /// Returns false when the browser should quit.
    pub fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) -> bool {
        if window_id != self.window.id() {
            self.overlays.handle_event(window_id, &event, &self.window);
            return true;
        }
        self.pump();

        match &event {
            WindowEvent::CloseRequested => {
                self.save_session();
                self.profile.flush(true);
                return false;
            },
            WindowEvent::Resized(size) => {
                self.gl_window.resize(*size);
                self.window.request_redraw();
            },
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
                if self.wants_quit() {
                    self.save_session();
                    self.profile.flush(true);
                    return false;
                }
                return true;
            },
            WindowEvent::Moved(_) => self.window.request_redraw(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                // Clicks inside a popup go to the popup's own window, so any
                // click here is outside it.
                if self.tab_menu.is_some() {
                    self.tab_menu = None;
                }
            },
            _ => {},
        }

        if self.handle_window_edges(&event) {
            return true;
        }

        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            if key.state == ElementState::Pressed && self.handle_shortcut(&shortcut_from(key, self.modifiers)) {
                self.window.request_redraw();
                return true;
            }
        }

        let Some(egui) = self.egui.as_mut() else { return true };
        let response = egui.on_window_event(&self.window, &event);
        if response.repaint {
            self.window.request_redraw();
        }
        true
    }

    /// Without the system title bar the window has no system borders either,
    /// so Ferrum resizes it from its edges. Returns true if it took the event.
    fn handle_window_edges(&mut self, event: &WindowEvent) -> bool {
        if cfg!(target_os = "macos") || !self.custom_title_bar_at_start || self.window.is_maximized() || self.window_fullscreen {
            if self.on_edge.take().is_some() {
                self.window.set_cursor(CursorIcon::Default);
            }
            return false;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let size = self.window.inner_size();
                let border = (5.0 * self.window.scale_factor()).max(4.0);
                let (x, y, w, h) = (position.x, position.y, size.width as f64, size.height as f64);
                let (left, right, top, bottom) = (x < border, x > w - border, y < border, y > h - border);
                use winit::window::ResizeDirection as R;
                let edge = match (left, right, top, bottom) {
                    (true, _, true, _) => Some(R::NorthWest),
                    (_, true, true, _) => Some(R::NorthEast),
                    (true, _, _, true) => Some(R::SouthWest),
                    (_, true, _, true) => Some(R::SouthEast),
                    (true, ..) => Some(R::West),
                    (_, true, ..) => Some(R::East),
                    (_, _, true, _) => Some(R::North),
                    (.., true) => Some(R::South),
                    _ => None,
                };
                if edge != self.on_edge {
                    let icon = match edge {
                        Some(R::West | R::East) => CursorIcon::EwResize,
                        Some(R::North | R::South) => CursorIcon::NsResize,
                        Some(R::NorthWest | R::SouthEast) => CursorIcon::NwseResize,
                        Some(R::NorthEast | R::SouthWest) => CursorIcon::NeswResize,
                        None => CursorIcon::Default,
                    };
                    self.window.set_cursor(icon);
                    self.on_edge = edge;
                }
                false
            },
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => match self.on_edge {
                Some(direction) => {
                    let _ = self.window.drag_resize_window(direction);
                    true
                },
                None => false,
            },
            _ => false,
        }
    }

    pub(crate) fn handle_shortcut(&mut self, s: &Shortcut) -> bool {
        let cmd = if cfg!(target_os = "macos") { s.meta } else { s.ctrl };
        let shift = s.shift;
        let key = if s.key.chars().count() == 1 { s.key.to_lowercase() } else { s.key.clone() };
        match key.as_str() {
            "t" if cmd && shift => self.reopen_closed_tab(),
            "t" if cmd => self.new_tab(),
            "n" if cmd && shift => self.open_private_window(),
            "w" if cmd => self.close_tab(self.active),
            "l" if cmd => self.focus_location = true,
            "f" if cmd => self.open_find(),
            "g" if cmd && self.find.open => self.find_step(if shift { -1 } else { 1 }),
            "F3" if self.find.open => self.find_step(if shift { -1 } else { 1 }),
            "p" if cmd => self.print(),
            "u" if cmd => self.view_source(),
            "r" if cmd && s.alt => self.toggle_reader(),
            "i" | "j" if cmd && (shift || s.alt) => self.toggle_devtools(),
            "F12" => self.toggle_devtools(),
            "F9" => self.toggle_reader(),
            "m" if cmd && !cfg!(target_os = "macos") => self.toggle_mute(self.active),
            "/" if cmd => self.open_internal(InternalPage::Shortcuts),
            "F6" => self.focus_location = true,
            "r" if cmd => self.reload(),
            "F5" => self.reload(),
            "d" if cmd => self.toggle_bookmark(),
            "b" if cmd && shift => {
                self.profile.settings.show_bookmarks_bar = !self.profile.settings.show_bookmarks_bar;
                self.profile.mark_dirty();
            },
            "h" if cmd => self.toggle_panel(SidePanel::History),
            "j" if cmd => self.toggle_panel(SidePanel::Downloads),
            "o" if cmd && shift => self.toggle_panel(SidePanel::Bookmarks),
            "," if cmd => self.open_settings(),
            "=" | "+" if cmd => self.zoom_step(true),
            "-" if cmd => self.zoom_step(false),
            "0" if cmd => self.set_zoom(1.0),
            "q" if cmd && cfg!(target_os = "macos") => self.request_quit(),
            "F11" => self.toggle_window_fullscreen(),
            "Escape" if self.site_info_open || self.menu_open || self.tab_menu.is_some() => self.close_popups(),
            "Escape" if self.find.open => self.close_find(),
            "Escape" if self.active_tab().reader.is_some() => self.toggle_reader(),
            "ArrowLeft" if s.alt => self.go_back(),
            "ArrowRight" if s.alt => self.go_forward(),
            "[" if cmd && cfg!(target_os = "macos") => self.go_back(),
            "]" if cmd && cfg!(target_os = "macos") => self.go_forward(),
            "Tab" if s.ctrl => {
                let n = self.tabs.len();
                let next = if shift { (self.active + n - 1) % n } else { (self.active + 1) % n };
                self.activate(next);
            },
            d if cmd && d.len() == 1 && ("1"..="9").contains(&d) => {
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

    /// Settings changed in the settings page.
    pub(crate) fn apply_engine_settings(&mut self) {
        self.downloads.dir = self.download_dir();
        self.blocker.enabled.set(self.profile.settings.block_trackers);
        *self.blocker.exceptions.borrow_mut() = self.profile.settings.blocker_exceptions.iter().cloned().collect();
    }

    pub(crate) fn download_dir(&self) -> Option<std::path::PathBuf> {
        let custom = self.profile.settings.download_dir.trim();
        if custom.is_empty() { dirs::download_dir() } else { Some(custom.into()) }
    }

    pub(crate) fn open_settings(&mut self) {
        self.open_internal(InternalPage::Settings);
    }

    pub(crate) fn open_internal(&mut self, page: InternalPage) {
        self.menu_open = false;
        if let Some(i) = self.tabs.iter().position(|t| t.internal == Some(page) && t.source.is_none()) {
            self.activate(i);
            return;
        }
        let id = self.new_id();
        let mut tab = Tab::new(id, self.profile.settings.default_zoom);
        tab.internal = Some(page);
        let at = (self.active + 1).min(self.tabs.len());
        self.tabs.insert(at, tab);
        self.activate(at);
    }

    pub(crate) fn toggle_panel(&mut self, panel: SidePanel) {
        self.panel = if self.panel == panel { SidePanel::None } else { panel };
        self.menu_open = false;
    }

    pub(crate) fn toggle_reader(&mut self) {
        self.menu_open = false;
        let tab = &mut self.tabs[self.active];
        if tab.reader.take().is_some() {
            return;
        }
        let Some(webview) = tab.webview.as_ref() else {
            self.toast("Open an article to use reader mode");
            return;
        };
        let id = tab.id;
        let proxy = self.proxy.clone();
        let result: Arc<Mutex<Option<Option<ReaderDoc>>>> = Arc::default();
        let slot = result.clone();
        let _ = webview.evaluate_script_with_callback(crate::reader::SCRIPT, move |value| {
            let json = serde_json::from_str::<String>(&value).unwrap_or(value);
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(ReaderDoc::parse(&json));
            }
            let _ = proxy.send_event(WakeUp);
        });
        self.pending_reader = Some((id, result));
    }

    pub(crate) fn view_source(&mut self) {
        self.menu_open = false;
        let Some(webview) = self.active_webview() else {
            self.toast("Open a page to view its source");
            return;
        };
        let url = webview.url().unwrap_or_default();
        let proxy = self.proxy.clone();
        let result: Arc<Mutex<Option<(String, String)>>> = Arc::default();
        let slot = result.clone();
        let _ = webview.evaluate_script_with_callback(
            "(document.doctype ? new XMLSerializer().serializeToString(document.doctype) + '\\n' : '') + document.documentElement.outerHTML",
            move |value| {
                let html = serde_json::from_str::<String>(&value).unwrap_or(value);
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some((url.clone(), html));
                }
                let _ = proxy.send_event(WakeUp);
            },
        );
        self.pending_source = Some(result);
    }

    fn finish_async_work(&mut self) {
        if let Some((id, slot)) = &self.pending_reader {
            let id = *id;
            let done = slot.lock().ok().and_then(|mut s| s.take());
            if let Some(doc) = done {
                self.pending_reader = None;
                match (doc, self.tab_index(id)) {
                    (Some(doc), Some(i)) => {
                        self.tabs[i].reader = Some(doc);
                        self.window.request_redraw();
                    },
                    (None, Some(_)) => self.toast("Reader mode isn't available for this page"),
                    _ => {},
                }
            }
        }
        if let Some(slot) = &self.pending_source {
            let done = slot.lock().ok().and_then(|mut s| s.take());
            if let Some((url, html)) = done {
                self.pending_source = None;
                let id = self.new_id();
                let mut tab = Tab::new(id, 1.0);
                tab.internal = Some(InternalPage::Source);
                tab.source = Some(Source::new(url, &html));
                let at = (self.active + 1).min(self.tabs.len());
                self.tabs.insert(at, tab);
                self.activate(at);
            }
        }
    }

    /// The engine's own developer tools (inspector, console, network…).
    pub(crate) fn toggle_devtools(&mut self) {
        self.menu_open = false;
        match self.active_webview() {
            Some(w) => {
                if w.is_devtools_open() {
                    w.close_devtools();
                } else {
                    w.open_devtools();
                }
            },
            None => self.toast("Open a page to use the developer tools"),
        }
    }

    pub(crate) fn toggle_mute(&mut self, index: usize) {
        let Some(tab) = self.tabs.get_mut(index) else { return };
        tab.muted = !tab.muted;
        if let Some(w) = &tab.webview {
            let _ = w.evaluate_script(&crate::pagescript::mute(tab.muted));
        }
        let msg = if tab.muted { "Tab muted" } else { "Tab unmuted" };
        self.toast(msg);
    }

    pub(crate) fn toggle_pin(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let pinned = !self.tabs[index].pinned;
        self.tabs[index].pinned = pinned;
        let pinned_count = self.tabs.iter().filter(|t| t.pinned).count();
        let target = if pinned { pinned_count - 1 } else { pinned_count };
        self.move_tab(index, target);
        self.profile.mark_dirty();
    }

    pub(crate) fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        let to = to.min(self.tabs.len() - 1);
        if from == to {
            return;
        }
        let active_tab = self.active;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = if active_tab == from {
            to
        } else if from < active_tab && to >= active_tab {
            active_tab - 1
        } else if from > active_tab && to <= active_tab {
            active_tab + 1
        } else {
            active_tab
        };
        self.profile.mark_dirty();
    }

    /// Unload tabs that haven't been looked at for a while.
    fn sleep_idle_tabs(&mut self) {
        if self.last_sleep_check.elapsed() < SLEEP_CHECK {
            return;
        }
        self.last_sleep_check = Instant::now();
        if !self.profile.settings.sleep_tabs {
            return;
        }
        let limit = Duration::from_secs(self.profile.settings.sleep_minutes.max(1) as u64 * 60);
        for i in 0..self.tabs.len() {
            let tab = &self.tabs[i];
            if i == self.active || tab.playing || tab.pinned || tab.loading() || tab.webview.is_none() || tab.last_used.elapsed() < limit {
                continue;
            }
            let (Some(url), title) = (tab.url(), tab.title()) else { continue };
            log::info!("Putting tab to sleep: {url}");
            let tab = &mut self.tabs[i];
            tab.webview = None;
            tab.reader = None;
            tab.shown = false;
            tab.pending = Some((url, title));
        }
    }

    fn maybe_check_for_updates(&mut self) {
        let s = &self.profile.settings;
        if !s.check_updates || self.profile.is_private() || crate::profile::now_secs().saturating_sub(s.last_update_check) < 24 * 3600 {
            return;
        }
        self.check_for_updates();
    }

    pub(crate) fn check_for_updates(&mut self) {
        self.profile.settings.last_update_check = crate::profile::now_secs();
        self.profile.mark_dirty();
        let proxy = self.proxy.clone();
        self.update = Some(crate::updates::check(crate::VERSION, Arc::new(move || {
            let _ = proxy.send_event(WakeUp);
        })));
    }

    pub(crate) fn update_status(&self) -> Option<UpdateStatus> {
        self.update.as_ref().and_then(|u| u.lock().ok().map(|s| s.clone()))
    }

    pub(crate) fn import_bookmarks(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Bookmarks file", &["html", "htm"]).pick_file() else { return };
        match std::fs::read_to_string(&path) {
            Ok(html) => {
                let items = crate::bookmarks_io::import(&html);
                let added = self.profile.import_bookmarks(items);
                self.toast(format!("Imported {added} bookmarks"));
            },
            Err(e) => self.toast(format!("Could not read the file: {e}")),
        }
    }

    pub(crate) fn export_bookmarks(&mut self) {
        let Some(path) = rfd::FileDialog::new().set_file_name("ferrum-bookmarks.html").add_filter("Bookmarks file", &["html"]).save_file() else { return };
        match std::fs::write(&path, crate::bookmarks_io::export(&self.profile.bookmarks)) {
            Ok(()) => self.toast(format!("Exported {} bookmarks", self.profile.bookmarks.len())),
            Err(e) => self.toast(format!("Could not save: {e}")),
        }
    }

    /// Switch tracker blocking off or on for the active tab's site, and reload it.
    pub(crate) fn toggle_site_blocking(&mut self) {
        let Some(host) = self.active_tab().page_url.as_ref().and_then(|u| u.host_str().map(str::to_owned)) else { return };
        let list = &mut self.profile.settings.blocker_exceptions;
        if let Some(i) = list.iter().position(|h| *h == host) {
            list.remove(i);
        } else {
            list.push(host);
        }
        self.profile.mark_dirty();
        self.apply_engine_settings();
        crate::adblock::prepare(&self.blocker.domains(), &self.profile.settings.blocker_exceptions, self.profile.settings.block_trackers);
        // Recreate the page so it gets the new page script and rules.
        if let Some(url) = self.active_tab().page_url.clone() {
            let id = self.tabs[self.active].id;
            let webview = self.create_webview(id, &url);
            let tab = &mut self.tabs[self.active];
            tab.webview = webview;
            tab.shown = false;
            tab.bounds = (0, 0, 0, 0);
        }
    }

    /// Delete cookies and storage for the active tab's site.
    pub(crate) fn clear_this_site(&mut self) {
        let Some(url) = self.active_tab().page_url.clone() else { return };
        let host = url.host_str().unwrap_or("").to_string();
        let base = base_domain(&host).to_string();
        if let Some(w) = self.active_webview() {
            for cookie in w.cookies().unwrap_or_default() {
                let domain = cookie.domain().unwrap_or("").trim_start_matches('.').to_string();
                if base_domain(&domain) == base {
                    let _ = w.delete_cookie(&cookie);
                }
            }
            let _ = w.evaluate_script("try { localStorage.clear(); sessionStorage.clear(); } catch (e) {}");
        }
        self.profile.settings.site_permissions.remove(&host);
        self.profile.mark_dirty();
        self.toast(format!("Cleared cookies and data for {base}"));
    }

    fn check_search_blocked(&mut self, id: u64) {
        let Some(i) = self.tab_index(id) else { return };
        let Some(search) = self.tabs[i].search.take() else { return };
        let Some(webview) = self.tabs[i].webview.as_ref() else { return };
        let slot = self.search_check.clone();
        let proxy = self.proxy.clone();
        let _ = webview.evaluate_script_with_callback(
            "(document.title + ' ' + location.host + ' ' + (document.body ? document.body.innerText.slice(0, 3000) : '')).toLowerCase()",
            move |value| {
                let text = serde_json::from_str::<String>(&value).unwrap_or(value);
                if looks_like_block_page(&text) {
                    if let Ok(mut slot) = slot.lock() {
                        *slot = Some((id, search.0, search.1.clone()));
                    }
                    let _ = proxy.send_event(WakeUp);
                }
            },
        );
    }

    pub(crate) fn search_with(&mut self, engine: SearchEngine, query: &str) {
        self.profile.settings.search_engine = engine;
        self.profile.settings.search_engine_chosen = true;
        self.profile.mark_dirty();
        self.search_blocked = None;
        self.navigate_input(query);
        self.toast(format!("Searching with {} from now on", engine.name()));
    }

    pub(crate) fn request_quit(&mut self) {
        self.quit_requested = true;
        self.window.request_redraw();
    }

    pub(crate) fn wants_quit(&self) -> bool {
        self.quit_requested
    }

    // ----- drawing ----------------------------------------------------------

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.pump();
        // Pages can change their address without loading (single-page apps).
        if self.last_url_poll.elapsed() > Duration::from_millis(500) {
            self.last_url_poll = Instant::now();
            if let Some(url) = self.active_webview().and_then(|w| w.url().ok()).and_then(|u| Url::parse(&u).ok()) {
                self.tabs[self.active].page_url = Some(url);
            }
        }
        self.sync_location();
        self.gl_window.make_current();

        let Some(mut egui) = self.egui.take() else { return };
        if let Ok(mut slot) = self.repaint_at.lock() {
            *slot = None;
        }
        self.overlay_requests.clear();
        let window = self.window.clone();
        egui.run(&window, |ui| self.draw(ui));
        let wants_keyboard = egui.egui_ctx.egui_wants_keyboard_input();

        let title = match self.active_webview() {
            Some(_) => format!("{} — {APP_NAME}", self.active_tab().title()),
            None => APP_NAME.to_string(),
        };
        let title = if self.profile.is_private() { format!("{title} (Private)") } else { title };
        if title != self.last_title {
            self.window.set_title(&title);
            self.last_title = title;
        }

        egui.paint(&self.window);
        self.gl_window.swap_buffers();
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| crate::timing("first window frame shown"));
        self.egui = Some(egui);

        self.layout_pages(wants_keyboard);
        self.draw_overlays(event_loop);
        self.gl_window.make_current();
    }

    /// Show the active tab's page over the page area, hide the others.
    fn layout_pages(&mut self, interface_has_keyboard: bool) {
        let r = self.content_rect;
        let p = self.ppp;
        let bounds = (
            (r.left() * p).round() as i32,
            (r.top() * p).round() as i32,
            (r.width() * p).round().max(1.0) as u32,
            (r.height() * p).round().max(1.0) as u32,
        );
        let page_visible = r.is_positive();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let Some(webview) = &tab.webview else { continue };
            let visible = i == self.active && page_visible && tab.internal.is_none() && tab.reader.is_none();
            if visible && tab.bounds != bounds {
                tab.bounds = bounds;
                let _ = webview.set_bounds(wry::Rect {
                    position: PhysicalPosition::new(bounds.0, bounds.1).into(),
                    size: PhysicalSize::new(bounds.2, bounds.3).into(),
                });
            }
            if visible != tab.shown {
                tab.shown = visible;
                let _ = webview.set_visible(visible);
                if visible && !interface_has_keyboard {
                    let _ = webview.focus();
                }
            }
        }
        // Typing in the address bar (or any field) takes the keyboard from the page.
        if interface_has_keyboard && !self.page_unfocused {
            self.focus_interface();
            self.page_unfocused = true;
        } else if !interface_has_keyboard {
            self.page_unfocused = false;
        }
    }

    /// Give the keyboard back to Ferrum's own window (from a page).
    fn focus_interface(&self) {
        #[cfg(target_os = "linux")]
        {
            // WebKitGTK keeps X11 input focus in its child window; move it to ours.
            use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
            if let (Ok(w), Ok(d)) = (self.window.window_handle(), self.window.display_handle()) {
                if let (RawWindowHandle::Xlib(w), RawDisplayHandle::Xlib(d)) = (w.as_raw(), d.as_raw()) {
                    if let (Ok(xlib), Some(display)) = (x11_dl::xlib::Xlib::open(), d.display) {
                        unsafe {
                            (xlib.XSetInputFocus)(display.as_ptr() as *mut _, w.window, x11_dl::xlib::RevertToParent, x11_dl::xlib::CurrentTime);
                            (xlib.XFlush)(display.as_ptr() as *mut _);
                        }
                    }
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        match self.tabs.iter().find_map(|t| t.webview.as_ref()) {
            Some(w) => {
                let _ = w.focus_parent();
            },
            None => self.window.focus_window(),
        }
    }

    /// Popups requested this frame, each in its own small window above the page.
    fn draw_overlays(&mut self, event_loop: &ActiveEventLoop) {
        let requests = std::mem::take(&mut self.overlay_requests);
        let mut overlays = std::mem::take(&mut self.overlays);
        let gl_window = self.gl_window.clone();
        overlays.sync(event_loop, &gl_window, &self.gl, &requests, self.ppp, &self.profile.settings);
        for kind in overlays.kinds() {
            overlays.draw(kind, &gl_window, |ui| self.draw_overlay(kind, ui));
        }
        self.overlays = overlays;
        self.overlay_requests = requests;
    }

    pub fn save_if_due(&mut self) {
        if self.profile.flush_due() {
            self.save_session();
            self.profile.flush(true);
        }
    }

    /// When the event loop must wake up next.
    pub fn next_wake(&mut self) -> Option<Instant> {
        self.save_if_due();
        self.sleep_idle_tabs();
        let sleep_check = (self.profile.settings.sleep_tabs && self.tabs.len() > 1).then(|| self.last_sleep_check + SLEEP_CHECK);
        // Linux: WebKitGTK runs on GTK's main loop, which main.rs pumps each time the loop wakes.
        let gtk = cfg!(target_os = "linux").then(|| Instant::now() + Duration::from_millis(16));
        [self.next_repaint(), self.profile.flush_at(), sleep_check, gtk].into_iter().flatten().min()
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        let mut slot = self.repaint_at.lock().ok()?;
        let at = (*slot)?;
        if at <= Instant::now() {
            *slot = None;
            self.window.request_redraw();
            None
        } else {
            Some(at)
        }
    }

    pub(crate) fn egui_ctx(&self) -> Option<egui::Context> {
        self.egui.as_ref().map(|e| e.egui_ctx.clone())
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.save_session();
        self.profile.flush(true);
        self.tabs.clear();
        self.overlays.destroy();
        if let Some(mut egui) = self.egui.take() {
            egui.destroy();
        }
    }
}

fn shortcut_from(key: &KeyEvent, m: ModifiersState) -> Shortcut {
    let name = match &key.logical_key {
        Key::Character(c) => c.to_string(),
        Key::Named(NamedKey::Space) => " ".into(),
        Key::Named(named) => format!("{named:?}"),
        _ => String::new(),
    };
    Shortcut { key: name, ctrl: m.control_key(), meta: m.super_key(), shift: m.shift_key(), alt: m.alt_key() }
}

/// Words found on the pages networks show instead of a blocked site.
pub(crate) fn looks_like_block_page(text: &str) -> bool {
    const SIGNS: [&str; 10] = [
        "internet positif", "internetpositif", "trustpositif", "internet baik", "diblokir",
        "komdigi", "kominfo", "situs ini tidak dapat diakses", "blocked by the government", "content blocked",
    ];
    SIGNS.iter().any(|s| text.contains(s))
}

#[cfg(test)]
mod tests {
    #[test]
    fn spots_block_pages() {
        assert!(super::looks_like_block_page("internet positif - situs ini diblokir"));
        assert!(super::looks_like_block_page("trustpositif.komdigi.go.id"));
        assert!(!super::looks_like_block_page("duckduckgo html results for rust"));
    }
}

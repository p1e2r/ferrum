// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The browser window: Servo webviews plus a native egui interface on one GL surface.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui_glow::EguiGlow;
use euclid::Scale;
use servo::{
    AllowOrDenyRequest, AuthenticationRequest, ConsoleLogLevel, MediaSessionEvent,
    MediaSessionPlaybackState, Notification, WebResourceLoad, WebResourceResponse, ColorPicker, ContextMenu, CookieSource, CreateNewWebViewRequest,
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
use crate::blocker::{Blocker, base_domain};
use crate::devtools::{Console, DevTools, Level, Source};
use crate::reader::ReaderDoc;
use crate::updates::UpdateStatus;
use crate::downloads::{Downloads, looks_like_download};
use crate::find::FindState;
use crate::print::{self, PrintJob};
use crate::keys::keyboard_event;
use crate::location::input_to_url;
use crate::profile::{NewTabPage, Profile, SavedTab};

pub const APP_NAME: &str = "Ferrum";
const SLEEP_CHECK: std::time::Duration = std::time::Duration::from_secs(60);
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
    /// "Leave this page?" from a page's `beforeunload` handler.
    Unload(AllowOrDenyRequest),
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
    Console(WebViewId, ConsoleLogLevel, String),
    Notify(Option<WebViewId>, String, String),
    Media(WebViewId, bool),
    LoadStarted(WebViewId),
}

/// Receives callbacks from Servo for every webview. Callbacks only queue work
/// (handled in `Browser::pump`) or ask for a redraw, so they stay cheap.
struct Delegate {
    window: Rc<Window>,
    rendering_context: Rc<OffscreenRenderingContext>,
    events: RefCell<Vec<Event>>,
    blocker: Rc<Blocker>,
}

impl Delegate {
    fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
        self.window.request_redraw();
    }
}

impl WebViewDelegate for Delegate {
    fn notify_new_frame_ready(&self, _: WebView) {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| crate::timing("first page frame from Servo"));
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
        } else if status == LoadStatus::Started {
            self.blocker.reset_count(webview.id());
            self.push(Event::LoadStarted(webview.id()));
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

    /// Ad and tracker blocking: cancel requests to listed servers from other sites.
    fn load_web_resource(&self, webview: WebView, load: WebResourceLoad) {
        let request = load.request();
        if request.is_for_main_frame {
            return;
        }
        let host = request.url.host_str().unwrap_or("").to_ascii_lowercase();
        let page = webview.url();
        let page_host = page.as_ref().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default();
        if self.blocker.should_block(&host, &page_host) {
            let url = request.url.clone().into_url();
            self.blocker.count_blocked(webview.id(), page.as_ref().map_or("", |u| u.as_str()), url.as_str());
            load.intercept(WebResourceResponse::new(url)).cancel();
        }
    }

    fn show_console_message(&self, webview: WebView, level: ConsoleLogLevel, message: String) {
        self.push(Event::Console(webview.id(), level, message));
    }

    fn show_notification(&self, webview: WebView, notification: Notification) {
        log::debug!("Notification from a page: {}", notification.title);
        self.push(Event::Notify(Some(webview.id()), notification.title, notification.body));
    }

    fn notify_media_session_event(&self, webview: WebView, event: MediaSessionEvent) {
        if let MediaSessionEvent::PlaybackStateChange(state) = event {
            self.push(Event::Media(webview.id(), matches!(state, MediaSessionPlaybackState::Playing)));
        }
    }

    fn request_unload(&self, webview: WebView, request: AllowOrDenyRequest) {
        self.push(Event::Control(Pending { webview: webview.id(), id: None, control: PageControl::Unload(request) }));
    }
}

/// Engine-wide callbacks. Some loads (images from the image cache, workers)
/// aren't tied to a tab; they're blocked using the page that referred them.
struct EngineDelegate {
    blocker: Rc<Blocker>,
    tabs: Rc<Delegate>,
}

impl servo::ServoDelegate for EngineDelegate {
    fn load_web_resource(&self, load: WebResourceLoad) {
        let request = load.request();
        if request.is_for_main_frame {
            return;
        }
        let Some(page_host) = request.referrer_url.as_ref().and_then(|u| u.host_str().map(str::to_ascii_lowercase)) else { return };
        let host = request.url.host_str().unwrap_or("").to_ascii_lowercase();
        if self.blocker.should_block(&host, &page_host) {
            let url = request.url.clone().into_url();
            load.intercept(WebResourceResponse::new(url)).cancel();
        }
    }

    fn show_notification(&self, notification: Notification) {
        self.tabs.push(Event::Notify(None, notification.title, notification.body));
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

    /// `ferrum://settings`, `ferrum://shortcuts`, or the `about:` aliases.
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

/// A tab is a live Servo webview, a page drawn by Ferrum (settings), or the
/// native start page.
pub struct Tab {
    pub webview: Option<WebView>,
    pub crashed: Option<String>,
    pub zoom: f32,
    pub favicon: Option<egui::TextureHandle>,
    pub favicon_dirty: bool,
    /// Set when Ferrum turned a typed host into an https:// URL, so a failed
    /// connection can fall back to http:// for sites without HTTPS.
    pub upgraded_to_https: bool,
    /// Automatic retries left for the current navigation.
    pub retries_left: u8,
    /// A restored tab that hasn't been opened yet: its page loads the first
    /// time the tab is selected, so start-up only loads the active tab.
    pub pending: Option<(Url, String)>,
    /// A page drawn by Ferrum, such as settings.
    pub internal: Option<InternalPage>,
    /// Pinned tabs stay at the left, show only their icon, and are kept by "close others".
    pub pinned: bool,
    pub muted: bool,
    /// The page is playing audio or video.
    pub playing: bool,
    pub console: Console,
    /// Reader mode, when on: the article extracted from the page.
    pub reader: Option<ReaderDoc>,
    /// For a page-source tab.
    pub source: Option<Source>,
    pub last_used: Instant,
}

impl Tab {
    fn new(webview: Option<WebView>, zoom: f32) -> Self {
        Self {
            webview,
            crashed: None,
            zoom,
            favicon: None,
            favicon_dirty: false,
            upgraded_to_https: false,
            retries_left: 1,
            pending: None,
            internal: None,
            pinned: false,
            muted: false,
            playing: false,
            console: Console::default(),
            reader: None,
            source: None,
            last_used: Instant::now(),
        }
    }

    fn restored(url: Url, title: String, zoom: f32) -> Self {
        Self { pending: Some((url, title)), ..Self::new(None, zoom) }
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
        if let Some(source) = &self.source {
            return Url::parse(&format!("view-source:{}", source.url)).ok();
        }
        if let Some(page) = self.internal {
            return Some(page.url());
        }
        if let Some((url, _)) = &self.pending {
            return Some(url.clone());
        }
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
    /// Where the suggestion list was last drawn, so clicks on it still count
    /// after the address bar loses focus.
    pub(crate) suggestion_rect: egui::Rect,
    pub(crate) start_query: String,

    // Chrome state
    pub(crate) panel: SidePanel,
    pub(crate) menu_open: bool,
    pub(crate) history_filter: String,
    pub(crate) page_fullscreen: bool,
    pub(crate) window_fullscreen: bool,
    pub(crate) toast: Option<(String, Instant)>,
    page_theme: Option<servo::Theme>,
    /// Hash of the look-related settings last applied to egui.
    pub(crate) style_key: u64,
    /// The start-page wallpaper, decoded once per path.
    pub(crate) start_image: Option<(String, Option<egui::TextureHandle>)>,
    pub(crate) find: FindState,
    pub(crate) print_job: Option<print::Job>,
    /// When the interface next needs a repaint (set by egui's repaint callback).
    repaint_at: Arc<Mutex<Option<Instant>>>,
    pub(crate) last_title: String,
    /// The engine's own User-Agent, used when no custom one is set.
    default_user_agent: String,
    /// Engine preferences last sent to Servo, to send only changes.
    engine_applied: Vec<(&'static str, servo::PrefValue)>,
    pub(crate) settings_section: crate::settings_page::Section,
    pub(crate) settings_query: String,
    pub(crate) blocker: Rc<Blocker>,
    pub(crate) devtools: DevTools,
    pub(crate) update: Option<Arc<Mutex<UpdateStatus>>>,
    /// The padlock / site information popup.
    pub(crate) site_info_open: bool,
    /// "Remember this decision" in permission prompts.
    pub(crate) remember_permission: bool,
    /// A web notification shown in the corner: (site, title, body, since).
    pub(crate) notification: Option<(String, String, String, Instant)>,
    proxy: EventLoopProxy<WakeUp>,
    last_sleep_check: Instant,
    pending_reader: Option<(WebViewId, Rc<RefCell<Option<Option<ReaderDoc>>>>)>,
    pending_source: Option<Rc<RefCell<Option<(String, String)>>>>,
    pending_screenshot: Option<Rc<RefCell<Option<Result<std::path::PathBuf, String>>>>>,
    pub(crate) last_screenshot: Option<std::path::PathBuf>,
    /// Whether the window was created without the system title bar. On macOS
    /// that can only be chosen when the window is made, so changes apply after a restart.
    pub(crate) custom_title_bar_at_start: bool,
    on_edge: Option<winit::window::ResizeDirection>,
    pub(crate) quit_requested: bool,
    console_results: Rc<RefCell<Vec<(WebViewId, Level, String)>>>,

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
        let window = Rc::new(event_loop.create_window(attributes).expect("create window"));
        crate::timing("window created");
        // Window/taskbar icon on Windows and Linux (macOS uses the app bundle's icon).
        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon-256.png")) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                window.set_window_icon(winit::window::Icon::from_rgba(rgba.into_raw(), w, h).ok());
            }
        }

        let display_handle = event_loop.display_handle().expect("display handle");
        let window_handle = window.window_handle().expect("window handle");
        let window_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                .expect("Could not create an OpenGL context for the window"),
        );
        // Pages render into an offscreen framebuffer that egui composites under the toolbar.
        let page_context = Rc::new(window_context.offscreen_context(window.inner_size()));
        page_context.make_current().expect("make GL context current");
        crate::timing("OpenGL contexts ready");

        // Cookies and site storage persist in the profile; private windows keep them in memory.
        let mut opts = Opts::default();
        match profile.engine_dir() {
            Some(dir) => {
                let _ = std::fs::create_dir_all(&dir);
                opts.config_dir = Some(dir);
            },
            None => opts.temporary_storage = true,
        }
        // WebGL 2 is off in Servo's defaults, but current three.js and most 3D
        // sites require it.
        let mut preferences = servo::Preferences::default();
        let default_user_agent = preferences.user_agent.clone();
        for (name, value) in engine_prefs(&profile.settings, &default_user_agent) {
            preferences.set_value(name, value);
        }
        preferences.dom_webgl2_enabled = true;
        // Servo disables "happy eyeballs" and splits this timeout across every
        // address a host resolves to; a longer budget avoids spurious
        // "client error (Connect)" on networks with slow or broken IPv6.
        preferences.network_connection_timeout = 30;
        let servo = ServoBuilder::default()
            .opts(opts)
            .preferences(preferences)
            .event_loop_waker(waker)
            .build();
        servo.setup_logging();

        crate::timing("Servo engine started");
        let egui = EguiGlow::new(event_loop, page_context.glow_gl_api(), None, None, false);
        crate::timing("interface ready");
        crate::ui::style(&egui.egui_ctx, &profile.settings);

        // egui tells us *when* it next needs to repaint (now, or after a delay for
        // tooltips, animations, the clock…). We record the earliest time and let the
        // event loop sleep until then, instead of redrawing continuously.
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

        let blocker = Rc::new(Blocker::new(profile.settings.block_trackers, &profile.settings.blocker_exceptions));
        // The full list is shared by private windows too; they just don't save anything else.
        blocker.load_list_in_background(Profile::default_dir());
        let delegate = Rc::new(Delegate {
            window: window.clone(),
            rendering_context: page_context.clone(),
            events: Default::default(),
            blocker: blocker.clone(),
        });
        servo.set_delegate(Rc::new(EngineDelegate { blocker: blocker.clone(), tabs: delegate.clone() }));

        let notify_window = proxy.clone();
        let downloads = Downloads::new(std::sync::Arc::new(move || {
            let _ = notify_window.send_event(WakeUp);
        }));

        let profile_settings_for_engine = profile.settings.clone();
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
            suggestion_rect: egui::Rect::NOTHING,
            start_query: String::new(),
            panel: SidePanel::None,
            menu_open: false,
            history_filter: String::new(),
            page_fullscreen: false,
            window_fullscreen: false,
            toast: None,
            page_theme: None,
            style_key: 0,
            start_image: None,
            find: FindState::default(),
            print_job: None,
            repaint_at,
            last_title: String::new(),
            engine_applied: engine_prefs(&profile_settings_for_engine, &default_user_agent),
            default_user_agent,
            settings_section: Default::default(),
            settings_query: String::new(),
            blocker,
            devtools: DevTools::default(),
            update: None,
            site_info_open: false,
            remember_permission: true,
            notification: None,
            proxy,
            last_sleep_check: Instant::now(),
            pending_reader: None,
            pending_source: None,
            pending_screenshot: None,
            last_screenshot: None,
            custom_title_bar_at_start: custom_title_bar,
            on_edge: None,
            quit_requested: false,
            console_results: Rc::default(),
            content_rect: egui::Rect::NOTHING,
            ppp: 1.0,
            mouse: None,
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
                    let mut tab = Tab::restored(url, saved.title, zoom);
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
            input_to_url(home, &self.profile.settings)
        }
    }

    /// Start-up tab: the home page if one is set, otherwise the start page.
    pub(crate) fn open_home_tab(&mut self) {
        let home = self.home_url();
        self.open_tab(home);
        if self.tabs[self.active].webview.is_none() {
            self.focus_location = true;
        }
    }

    /// A new tab, following the "New tabs open" setting.
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
        // A restored tab loads its page the first time it's shown.
        if let Some((url, _)) = self.tabs.get_mut(self.active).and_then(|t| t.pending.take()) {
            if let Some(page) = InternalPage::from_url(&url) {
                self.tabs[self.active].internal = Some(page);
            } else {
                let zoom = self.tabs[self.active].zoom;
                let webview = self.create_webview(url);
                webview.set_page_zoom(zoom);
                self.tabs[self.active].webview = Some(webview);
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.last_used = Instant::now();
        }
        self.site_info_open = false;
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

    /// Remove a tab without re-activating (used when closing several at once).
    pub(crate) fn close_tab_quietly(&mut self, index: usize) {
        if index >= self.tabs.len() || self.tabs.len() == 1 {
            return;
        }
        let tab = self.tabs.remove(index);
        if let Some(url) = tab.url() {
            self.closed_tabs.push(url.to_string());
        }
        if let Some(webview) = &tab.webview {
            self.controls.retain(|c| c.webview != webview.id());
        }
        if self.active >= self.tabs.len() || index < self.active {
            self.active = self.active.saturating_sub(1);
        }
    }

    pub(crate) fn insert_tab_after(&mut self, index: usize, webview: WebView, zoom: f32) {
        webview.set_page_zoom(zoom);
        self.tabs.insert(index + 1, Tab::new(Some(webview), zoom));
        self.activate(index + 1);
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
        if let Some(url) = input_to_url(input, &self.profile.settings) {
            let upgraded = url.scheme() == "https" && !input.trim().to_ascii_lowercase().starts_with("https://");
            self.navigate(url);
            self.tabs[self.active].upgraded_to_https = upgraded;
        }
    }

    pub(crate) fn navigate(&mut self, url: Url) {
        self.find.reset();
        self.editing = false;
        self.suggestion_index = None;
        if let Some(page) = InternalPage::from_url(&url) {
            let tab = &mut self.tabs[self.active];
            tab.webview = None;
            tab.crashed = None;
            tab.favicon = None;
            tab.source = None;
            tab.reader = None;
            tab.internal = Some(page);
            self.activate(self.active);
            return;
        }
        let tab = &mut self.tabs[self.active];
        tab.internal = None;
        tab.source = None;
        tab.reader = None;
        tab.crashed = None;
        tab.upgraded_to_https = false;
        tab.retries_left = 1;
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
        self.location = self.tabs.get(self.active).and_then(|t| t.url()).map(|url| url.to_string()).unwrap_or_default();
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
        // The interface's current mode (set by the theme or the light/dark setting).
        let ui_theme = match ctx.theme() {
            egui::Theme::Dark => servo::Theme::Dark,
            egui::Theme::Light => servo::Theme::Light,
        };
        // Servo darkens even pages that don't support dark mode, so this is opt-in.
        let theme = if self.profile.settings.dark_websites { ui_theme } else { servo::Theme::Light };
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
        // Start-page tabs have no URL and aren't saved, so count the active
        // tab's position among the saved ones.
        let mut tabs: Vec<SavedTab> = Vec::new();
        let mut active = 0;
        for (i, t) in self.tabs.iter().enumerate() {
            if i == self.active {
                active = tabs.len();
            }
            // Page-source tabs aren't kept: their text isn't saved.
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
                Event::Control(pending) => self.add_control(pending),
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
                    if self.recover_failed_load(&webview) {
                        continue;
                    }
                    self.profile.mark_dirty();
                    if self.active_webview().is_some_and(|w| w.id() == webview.id()) {
                        // A new document: old highlights are gone, search again if the bar is open.
                        self.find.reset();
                    }
                    if let Some(url) = webview.url() {
                        let title = webview.page_title();
                        self.profile.record_visit(&url, title.as_deref());
                    }
                    if self.tab_index(webview.id()).is_some_and(|i| self.tabs[i].muted) {
                        webview.evaluate_javascript(mute_script(true), |_| {});
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
                Event::Console(id, level, message) => {
                    if let Some(i) = self.tab_index(id) {
                        self.tabs[i].console.push(level.into(), message);
                    }
                },
                Event::Notify(id, title, body) => {
                    let site = id.and_then(|id| self.tab_index(id)).and_then(|i| self.tabs[i].url()).and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
                    self.notification = Some((site, title, body, Instant::now()));
                },
                Event::Media(id, playing) => {
                    if let Some(i) = self.tab_index(id) {
                        self.tabs[i].playing = playing;
                    }
                },
                Event::LoadStarted(id) => {
                    if let Some(i) = self.tab_index(id) {
                        // A new page: reader mode and the playing indicator start over.
                        self.tabs[i].reader = None;
                        self.tabs[i].playing = false;
                        self.tabs[i].console.clear();
                    }
                },
            }
        }
        let results: Vec<_> = self.console_results.borrow_mut().drain(..).collect();
        if !results.is_empty() {
            self.window.request_redraw();
        }
        for (id, level, text) in results {
            if let Some(i) = self.tab_index(id) {
                self.tabs[i].console.push(level, text);
            }
        }
        self.finish_async_work();
        self.sleep_idle_tabs();
        if let Some(job) = &self.print_job {
            print::advance(job, &self.window);
        }
        self.save_if_due();
    }

    /// Servo shows a plain "Error loading page" document when it can't connect.
    /// Fall back from an automatic https:// upgrade to http://, or retry once,
    /// before leaving the error showing. Returns true if a new load was started.
    fn recover_failed_load(&mut self, webview: &WebView) -> bool {
        let Some(i) = self.tab_index(webview.id()) else { return false };
        let https_only = self.profile.settings.https_only;
        let failed = webview.page_title().as_deref() == Some("Error loading page");
        let tab = &mut self.tabs[i];
        if !failed {
            tab.retries_left = 1;
            return false;
        }
        let Some(url) = webview.url() else { return false };
        if tab.upgraded_to_https && !https_only && url.scheme() == "https" {
            tab.upgraded_to_https = false;
            let mut http = url.clone();
            if http.set_scheme("http").is_ok() {
                log::info!("HTTPS connection to {url} failed; trying {http}");
                webview.load(http);
                return true;
            }
        }
        if tab.retries_left > 0 {
            tab.retries_left -= 1;
            log::info!("Loading {url} failed; retrying once");
            webview.load(url);
            return true;
        }
        false
    }

    pub(crate) fn start_print(&mut self) {
        if let Some(webview) = self.active_webview().cloned() {
            let title = self.active_tab().title();
            self.menu_open = false;
            self.print_job = Some(PrintJob::new(webview, title));
            self.window.request_redraw();
        } else {
            self.toast("Open a page to print it");
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
        if let Some(webview) = self.active_webview().cloned() {
            self.find.step(&webview, &self.window, delta);
        }
    }

    pub(crate) fn close_find(&mut self) {
        let webview = self.active_webview().cloned();
        self.find.close(webview.as_ref(), &self.window);
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
            // Trackpad pinch: zoom the page like a phone does, around the pointer.
            WindowEvent::PinchGesture { delta, .. } if delta.is_finite() => {
                if let (Some(webview), Some(pos)) = (self.active_webview().cloned(), self.mouse.filter(|p| self.in_content(*p))) {
                    webview.adjust_pinch_zoom(1.0 + *delta as f32, self.page_point(pos));
                    self.window.request_redraw();
                }
                return true;
            },
            _ => {},
        }

        if self.handle_window_edges(&event) {
            return true;
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

    /// Without the system title bar the window has no system borders either, so
    /// Ferrum resizes it from its edges itself. Returns true if it took the event.
    fn handle_window_edges(&mut self, event: &WindowEvent) -> bool {
        if cfg!(target_os = "macos") || !self.custom_title_bar_at_start || self.window.is_maximized() || self.window_fullscreen || self.page_fullscreen {
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
            (_, "t") if cmd => self.new_tab(),
            (_, "n") if cmd && shift => self.open_private_window(),
            (_, "w") if cmd => self.close_tab(self.active),
            (_, "l") if cmd => self.focus_location = true,
            (_, "f") if cmd => self.open_find(),
            (_, "g") if cmd && self.find.open => self.find_step(if shift { -1 } else { 1 }),
            (Key::Named(NamedKey::F3), _) if self.find.open => self.find_step(if shift { -1 } else { 1 }),
            (_, "p") if cmd => self.start_print(),
            (_, "s") if cmd && shift => self.take_screenshot(),
            (_, "u") if cmd => self.view_source(),
            (_, "r") if cmd && m.alt_key() => self.toggle_reader(),
            (_, "i" | "j") if cmd && (shift || m.alt_key()) => self.toggle_devtools(),
            (Key::Named(NamedKey::F12), _) => self.toggle_devtools(),
            (Key::Named(NamedKey::F9), _) => self.toggle_reader(),
            (_, "m") if cmd && !cfg!(target_os = "macos") => self.toggle_mute(self.active),
            (_, "/") if cmd => self.open_internal(InternalPage::Shortcuts),
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
            (_, ",") if cmd => self.open_settings(),
            (_, "=" | "+") if cmd => self.zoom_step(true),
            (_, "-") if cmd => self.zoom_step(false),
            (_, "0") if cmd => self.set_zoom(1.0),
            (_, "q") if cmd && cfg!(target_os = "macos") => {
                self.save_session();
                self.profile.flush(true);
                std::process::exit(0);
            },
            (Key::Named(NamedKey::F11), _) => self.toggle_window_fullscreen(),
            (Key::Named(NamedKey::Escape), _) if self.print_job.is_some() => self.print_job = None,
            (Key::Named(NamedKey::Escape), _) if self.site_info_open => self.site_info_open = false,
            (Key::Named(NamedKey::Escape), _) if self.active_tab().reader.is_some() => self.toggle_reader(),
            (Key::Named(NamedKey::Escape), _) if self.find.open => self.close_find(),
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

    /// Queue a page request, answering permission requests at once when the
    /// user asked Ferrum to remember the answer for that site.
    fn add_control(&mut self, pending: Pending) {
        if let PageControl::Permission(request) = &pending.control {
            let host = self.tab_index(pending.webview).and_then(|i| self.tabs[i].url()).and_then(|u| u.host_str().map(str::to_owned));
            let key = feature_key(request.feature());
            let remembered = host.and_then(|h| self.profile.settings.site_permissions.get(&h)?.get(key).copied());
            if let Some(allowed) = remembered {
                let PageControl::Permission(request) = pending.control else { unreachable!() };
                if allowed { request.allow() } else { request.deny() }
                return;
            }
        }
        self.controls.push(pending);
    }

    /// Remember a permission answer for the active tab's site.
    pub(crate) fn remember_permission_answer(&mut self, feature: servo::PermissionFeature, allowed: bool) {
        let Some(host) = self.active_tab().url().and_then(|u| u.host_str().map(str::to_owned)) else { return };
        self.profile.settings.site_permissions.entry(host).or_default().insert(feature_key(feature).to_string(), allowed);
        self.profile.mark_dirty();
    }

    /// Switch to an internal page's tab, opening one if needed.
    pub(crate) fn open_internal(&mut self, page: InternalPage) {
        self.menu_open = false;
        if let Some(i) = self.tabs.iter().position(|t| t.internal == Some(page) && t.source.is_none()) {
            self.activate(i);
            return;
        }
        let mut tab = Tab::new(None, self.profile.settings.default_zoom);
        tab.internal = Some(page);
        self.tabs.insert(self.active + 1, tab);
        self.activate(self.active + 1);
    }

    /// Turn reader mode on or off for the active tab.
    pub(crate) fn toggle_reader(&mut self) {
        self.menu_open = false;
        let tab = &mut self.tabs[self.active];
        if tab.reader.take().is_some() {
            if let Some(w) = &tab.webview {
                w.show();
            }
            return;
        }
        let Some(webview) = tab.webview.clone() else {
            self.toast("Open an article to use reader mode");
            return;
        };
        let id = webview.id();
        let proxy = self.proxy.clone();
        let result: Rc<RefCell<Option<Option<ReaderDoc>>>> = Rc::default();
        let slot = result.clone();
        webview.evaluate_javascript(crate::reader::SCRIPT, move |value| {
            let doc = match value {
                Ok(servo::JSValue::String(json)) => ReaderDoc::parse(&json),
                _ => None,
            };
            *slot.borrow_mut() = Some(doc);
            let _ = proxy.send_event(WakeUp);
        });
        self.pending_reader = Some((id, result));
    }

    /// Open the active page's HTML source in a new tab.
    pub(crate) fn view_source(&mut self) {
        self.menu_open = false;
        let Some(webview) = self.active_webview().cloned() else {
            self.toast("Open a page to view its source");
            return;
        };
        let url = webview.url().map(|u| u.to_string()).unwrap_or_default();
        let proxy = self.proxy.clone();
        let result: Rc<RefCell<Option<(String, String)>>> = Rc::default();
        let slot = result.clone();
        webview.evaluate_javascript(
            "(document.doctype ? new XMLSerializer().serializeToString(document.doctype) + '\\n' : '') + document.documentElement.outerHTML",
            move |value| {
                let html = match value {
                    Ok(servo::JSValue::String(s)) => s,
                    Ok(other) => crate::devtools::format_value(&other),
                    Err(e) => format!("Could not read the page source: {e:?}"),
                };
                *slot.borrow_mut() = Some((url, html));
                let _ = proxy.send_event(WakeUp);
            },
        );
        self.pending_source = Some(result);
    }

    /// Save a PNG of the visible page to the downloads folder.
    pub(crate) fn take_screenshot(&mut self) {
        self.menu_open = false;
        let Some(webview) = self.active_webview().cloned() else {
            self.toast("Open a page to take a screenshot");
            return;
        };
        let dir = dirs::picture_dir().or_else(|| self.download_dir()).or_else(dirs::home_dir).unwrap_or_else(std::env::temp_dir);
        let name = format!("Screenshot {}.png", chrono::Local::now().format("%Y-%m-%d at %H.%M.%S"));
        let path = dir.join(name);
        let proxy = self.proxy.clone();
        let result: Rc<RefCell<Option<Result<std::path::PathBuf, String>>>> = Rc::default();
        let slot = result.clone();
        webview.take_screenshot(None, move |shot| {
            let outcome = match shot {
                Ok(image) => {
                    let _ = std::fs::create_dir_all(&dir);
                    image::DynamicImage::ImageRgba8(image).save(&path).map(|_| path).map_err(|e| e.to_string())
                },
                Err(e) => Err(format!("{e:?}")),
            };
            *slot.borrow_mut() = Some(outcome);
            let _ = proxy.send_event(WakeUp);
        });
        self.pending_screenshot = Some(result);
        self.toast("Taking screenshot…");
    }

    /// Pick up results of reader / source / screenshot requests.
    fn finish_async_work(&mut self) {
        if let Some((id, slot)) = &self.pending_reader {
            let id = *id;
            let done = slot.borrow_mut().take();
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
            let done = slot.borrow_mut().take();
            if let Some((url, html)) = done {
                self.pending_source = None;
                let mut tab = Tab::new(None, 1.0);
                tab.internal = Some(InternalPage::Source);
                tab.source = Some(Source::new(url, &html));
                self.tabs.insert(self.active + 1, tab);
                self.activate(self.active + 1);
            }
        }
        if let Some(slot) = &self.pending_screenshot {
            let done = slot.borrow_mut().take();
            if let Some(outcome) = done {
                self.pending_screenshot = None;
                match outcome {
                    Ok(path) => {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.toast(format!("Saved {name} to {}", path.parent().map(|p| p.display().to_string()).unwrap_or_default()));
                        self.last_screenshot = Some(path);
                    },
                    Err(e) => self.toast(format!("Screenshot failed: {e}")),
                }
            }
        }
    }

    pub(crate) fn toggle_devtools(&mut self) {
        self.menu_open = false;
        self.devtools.open = !self.devtools.open;
        self.devtools.focus = self.devtools.open;
    }

    /// Run code typed into the console in the active page.
    pub(crate) fn run_in_console(&mut self, code: String) {
        let Some(webview) = self.active_webview().cloned() else { return };
        let i = self.active;
        self.tabs[i].console.push(Level::Input, code.clone());
        let id = webview.id();
        let proxy = self.proxy.clone();
        let results = self.console_results.clone();
        webview.evaluate_javascript(code, move |value| {
            let (level, text) = match value {
                Ok(v) => (Level::Result, crate::devtools::format_value(&v)),
                Err(e) => (Level::Error, format!("{e:?}")),
            };
            results.borrow_mut().push((id, level, text));
            let _ = proxy.send_event(WakeUp);
        });
    }

    pub(crate) fn toggle_mute(&mut self, index: usize) {
        let Some(tab) = self.tabs.get_mut(index) else { return };
        tab.muted = !tab.muted;
        if let Some(w) = &tab.webview {
            w.evaluate_javascript(mute_script(tab.muted), |_| {});
        }
        let msg = if tab.muted { "Tab muted" } else { "Tab unmuted" };
        self.toast(msg);
    }

    /// Pin or unpin a tab. Pinned tabs are kept together at the left.
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

    /// Move a tab to a new position (drag and drop), keeping the same tab active.
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

    /// Unload tabs that haven't been looked at for a while. They keep their
    /// place, title and icon and load again when selected.
    fn sleep_idle_tabs(&mut self) {
        if self.last_sleep_check.elapsed() < SLEEP_CHECK {
            return;
        }
        self.last_sleep_check = Instant::now();
        if !self.profile.settings.sleep_tabs {
            return;
        }
        let limit = std::time::Duration::from_secs(self.profile.settings.sleep_minutes.max(1) as u64 * 60);
        for i in 0..self.tabs.len() {
            let tab = &self.tabs[i];
            let busy = tab.playing || tab.pinned || tab.loading() || self.controls.iter().any(|c| Some(c.webview) == tab.webview.as_ref().map(|w| w.id()));
            if i == self.active || busy || tab.webview.is_none() || tab.last_used.elapsed() < limit {
                continue;
            }
            let (Some(url), title) = (tab.url(), tab.title()) else { continue };
            log::info!("Putting tab to sleep: {url}");
            let tab = &mut self.tabs[i];
            tab.webview = None;
            tab.reader = None;
            tab.pending = Some((url, title));
        }
    }

    /// Check for a newer release at start-up, at most once a day.
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
        let Some(host) = self.active_tab().url().and_then(|u| u.host_str().map(str::to_owned)) else { return };
        let list = &mut self.profile.settings.blocker_exceptions;
        if let Some(i) = list.iter().position(|h| *h == host) {
            list.remove(i);
        } else {
            list.push(host);
        }
        self.profile.mark_dirty();
        self.apply_engine_settings();
        self.reload();
    }

    /// Delete cookies and storage for the active tab's site.
    pub(crate) fn clear_this_site(&mut self) {
        let Some(host) = self.active_tab().url().and_then(|u| u.host_str().map(str::to_owned)) else { return };
        let base = base_domain(&host).to_string();
        let manager = self.servo.site_data_manager();
        let names: Vec<String> = manager.site_data(StorageType::all()).iter().map(|s| s.name()).filter(|n| base_domain(n) == base).collect();
        let mut refs: Vec<&str> = names.iter().map(String::as_str).collect();
        refs.push(&base);
        manager.clear_site_data(&refs, StorageType::all());
        self.profile.settings.site_permissions.remove(&host);
        self.profile.mark_dirty();
        self.toast(format!("Cleared cookies and data for {base}"));
    }

    /// Send changed engine settings to Servo. They apply to pages loaded afterwards.
    pub(crate) fn apply_engine_settings(&mut self) {
        let prefs = engine_prefs(&self.profile.settings, &self.default_user_agent);
        for (name, value) in &prefs {
            let changed = self.engine_applied.iter().find(|(n, _)| n == name).is_none_or(|(_, v)| v != value);
            if changed {
                self.servo.set_preference(name, value.clone());
            }
        }
        self.engine_applied = prefs;
        self.downloads.dir = self.download_dir();
        self.blocker.enabled.set(self.profile.settings.block_trackers);
        *self.blocker.exceptions.borrow_mut() = self.profile.settings.blocker_exceptions.iter().cloned().collect();
    }

    /// The folder downloads go to.
    pub(crate) fn download_dir(&self) -> Option<std::path::PathBuf> {
        let custom = self.profile.settings.download_dir.trim();
        if custom.is_empty() { dirs::download_dir() } else { Some(custom.into()) }
    }

    /// Switch to the settings tab, opening one if needed.
    pub(crate) fn open_settings(&mut self) {
        self.open_internal(InternalPage::Settings);
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
        let blocked = self.has_popup() || self.menu_open || self.print_job.is_some();

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
        if let Ok(mut slot) = self.repaint_at.lock() {
            *slot = None;
        }
        let window = self.window.clone();
        egui.run(&window, |ui| self.draw(ui));

        let title = match self.active_webview() {
            Some(_) => format!("{} — {APP_NAME}", self.active_tab().title()),
            None => APP_NAME.to_string(),
        };
        let title = if self.profile.is_private() { format!("{title} (Private)") } else { title };
        // Setting the title is surprisingly expensive on macOS; only do it when it changes.
        if title != self.last_title {
            self.window.set_title(&title);
            self.last_title = title;
        }

        self.page_context.make_current().expect("make GL context current");
        self.window_context.prepare_for_rendering();
        egui.paint(&self.window);
        self.window_context.present();
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| crate::timing("first window frame shown"));
        self.egui = Some(egui);
    }

    /// Write the profile and session once changes have settled. Saving the session
    /// together with the profile means tabs survive even an abrupt quit (Cmd+Q from
    /// the macOS menu bar, or a crash).
    pub fn save_if_due(&mut self) {
        if self.profile.flush_due() {
            self.save_session();
            self.profile.flush(true);
        }
    }

    /// When the event loop must wake up next: a scheduled repaint or a pending save.
    pub fn next_wake(&mut self) -> Option<Instant> {
        self.save_if_due();
        self.sleep_idle_tabs();
        let sleep_check = (self.profile.settings.sleep_tabs && self.tabs.len() > 1).then(|| self.last_sleep_check + SLEEP_CHECK);
        [self.next_repaint(), self.profile.flush_at(), sleep_check].into_iter().flatten().min()
    }

    /// Called when the event loop is about to sleep: redraw now if a repaint is
    /// due, otherwise return when the next one is (so the loop can wake for it).
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

/// Engine preferences that come from Ferrum's settings.
pub(crate) fn engine_prefs(settings: &crate::profile::Settings, default_user_agent: &str) -> Vec<(&'static str, servo::PrefValue)> {
    use servo::PrefValue::{Bool, Int, Str};
    let user_agent = settings.user_agent.trim();
    vec![
        ("network_enforce_tls_enabled", Bool(settings.https_only)),
        ("dom_notification_enabled", Bool(settings.allow_notifications)),
        ("dom_geolocation_enabled", Bool(settings.allow_location)),
        ("fonts_default_size", Int(settings.font_size.clamp(9, 32) as i64)),
        ("user_agent", Str(if user_agent.is_empty() { default_user_agent.to_string() } else { user_agent.to_string() })),
    ]
}

/// The name a permission is remembered under in settings.
pub(crate) fn feature_key(feature: servo::PermissionFeature) -> &'static str {
    use servo::PermissionFeature as F;
    match feature {
        F::Geolocation => "location",
        F::Notifications | F::Push => "notifications",
        F::Camera => "camera",
        F::Microphone => "microphone",
        F::Speaker => "speaker",
        F::Midi => "midi",
        F::Bluetooth => "bluetooth",
        F::PersistentStorage => "storage",
        F::ScreenWakeLock(_) => "wake-lock",
        F::Gamepad => "gamepad",
        F::DeviceInfo => "device-info",
        F::BackgroundSync => "background-sync",
    }
}

/// Mutes or unmutes every `<audio>`/`<video>` in a page, including ones that
/// start playing later.
fn mute_script(muted: bool) -> String {
    format!(
        "(function(m){{window.__ferrumMuted=m;document.querySelectorAll('video,audio').forEach(function(e){{e.muted=m}});\
         if(!window.__ferrumMuteHook){{window.__ferrumMuteHook=true;document.addEventListener('play',function(e){{\
         if(window.__ferrumMuted&&'muted' in e.target)e.target.muted=true}},true)}}}})({muted})"
    )
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

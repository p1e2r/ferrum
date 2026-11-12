// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors

//! The settings page (`ferrum://settings`), drawn natively in its own tab.
//!
//! Every setting is listed once in `ITEMS` with its section, label,
//! description and search keywords. The page shows one section at a time, or
//! every matching setting while searching. `Browser::setting_control` draws
//! the control for each one.

use egui::{Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use url::Url;

use crate::updates::UpdateStatus;

/// The web engine pages are shown with on this platform.
const ENGINE_NAME: &str = if cfg!(target_os = "macos") {
    "WebKit (Safari)"
} else if cfg!(target_os = "windows") {
    "WebView2 (Edge / Chromium)"
} else {
    "WebKitGTK"
};

use crate::browser::{APP_NAME, Browser};
use crate::profile::{CloseButtons, EnergySaver, TabShape, DEFAULT_ACCENT, NewTabPage, SearchEngine, Settings, StartBackground, TabLayout, TabWidth, ThemeChoice};
use crate::theme::{ThemePreset, mix};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Appearance,
    Tabs,
    StartPage,
    Search,
    Startup,
    Downloads,
    Privacy,
    WebContent,
    Advanced,
    About,
}

impl Section {
    const ALL: [Section; 10] = [
        Section::Appearance,
        Section::Tabs,
        Section::StartPage,
        Section::Search,
        Section::Startup,
        Section::Downloads,
        Section::Privacy,
        Section::WebContent,
        Section::Advanced,
        Section::About,
    ];

    fn name(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Tabs => "Tabs & toolbar",
            Section::StartPage => "Start page",
            Section::Search => "Search & address bar",
            Section::Startup => "On start-up",
            Section::Downloads => "Downloads",
            Section::Privacy => "Privacy & security",
            Section::WebContent => "Web content",
            Section::Advanced => "Advanced",
            Section::About => "About Ferrum",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Section::Appearance => "Themes, colours and the size of the interface.",
            Section::Tabs => "Where tabs go and which buttons the toolbar shows.",
            Section::StartPage => "What the start page shows when you open a new tab.",
            Section::Search => "Your search engine and what the address bar suggests.",
            Section::Startup => "What opens when Ferrum starts and when you open a new tab.",
            Section::Downloads => "Where downloaded files are saved.",
            Section::Privacy => "Secure connections, site permissions and clearing your data. Changes apply to pages you open or reload.",
            Section::WebContent => "Text size and zoom for web pages. Changes apply to pages you open or reload.",
            Section::Advanced => "For troubleshooting and testing websites.",
            Section::About => "",
        }
    }
}

/// Every setting on the page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Lite,
    EnergySaver,
    Theme,
    Mode,
    Accent,
    UiScale,
    Density,
    Roundness,
    TabLayout,
    TabWidth,
    TabShape,
    TabCloseButtons,
    NewTabPosition,
    LastTab,
    ForwardButton,
    ReloadButton,
    ReaderButton,
    LoadingBar,
    DomainOnly,
    Animations,
    StartSearch,
    ClockFormat,
    StartTiles,
    BookmarksBar,
    HomeButton,
    BookmarkButton,
    DownloadsButton,
    StartTitle,
    StartBackground,
    StartColor,
    Gradient,
    StartImage,
    StartVideo,
    StartClock,
    StartBookmarks,
    StartFrequent,
    SearchEngine,
    CustomSearch,
    Suggestions,
    RestoreSession,
    NewTabPage,
    HomePage,
    DownloadFolder,
    AskWhereToSave,
    HttpsOnly,
    DarkWebsites,
    Notifications,
    ClearHistory,
    ClearSiteData,
    DefaultZoom,
    UserAgent,
    DataFolder,
    ResetAll,
    About,
    BlockTrackers,
    BlockerExceptions,
    SitePermissions,
    SleepTabs,
    KeepClosedTab,
    BookmarkFile,
    ImportBrowser,
    CheckUpdates,
    TitleBar,
}

struct Item {
    section: Section,
    setting: Setting,
    label: &'static str,
    desc: &'static str,
    /// Extra words the search box should find this by.
    keywords: &'static str,
    /// Put the control under the text instead of beside it.
    wide: bool,
}

const fn item(section: Section, setting: Setting, label: &'static str, desc: &'static str, keywords: &'static str, wide: bool) -> Item {
    Item { section, setting, label, desc, keywords, wide }
}

use Section as S;
use Setting as T;

const ITEMS: &[Item] = &[
    item(S::Appearance, T::Lite, "Lite mode", "For slow or older computers. Turns off animations and fancy start-page backgrounds, puts unused tabs to sleep after 5 minutes, stops videos playing by themselves, and keeps the web engine's memory use low.", "lite slow old low end performance fast speed lag memory ram battery optimize optimise", false),
    item(S::Appearance, T::EnergySaver, "Energy saver", "Saves battery: pages and Ferrum draw at 30 frames a second, unused tabs sleep after 2 minutes, videos in other tabs pause, and no animations. A green leaf shows in the toolbar while it's on.", "energy saver battery power save fps frame rate 30 sleep laptop unplugged", false),
    item(S::Appearance, T::Theme, "Theme", "Colours for the whole interface. Choosing a theme also sets its accent colour.", "colour color palette dark light skin look", true),
    item(S::Appearance, T::Mode, "Light or dark", "Works with every theme. System follows your computer's setting.", "dark mode light mode system appearance night", false),
    item(S::Appearance, T::Accent, "Accent colour", "Used for highlights, selected items and the loading bar.", "color highlight tint", true),
    item(S::Appearance, T::UiScale, "Interface size", "Size of tabs, toolbar and menus. Doesn't affect pages; use page zoom for that.", "scale zoom ui bigger smaller text size", false),
    item(S::Appearance, T::Density, "Density", "Compact fits more on screen.", "compact comfortable spacing padding", false),
    item(S::Appearance, T::Animations, "Animations", "Switches, panels and fades. Turn off on slow computers or if you prefer less motion.", "motion reduce animate speed performance", false),
    item(S::Appearance, T::Roundness, "Corner rounding", "How rounded buttons, tabs and fields are.", "radius corners square", false),
    item(S::Tabs, T::TitleBar, "Tabs in the title bar", "Like Chrome: no system title bar, the tabs sit at the top of the window with Ferrum's own minimise, maximise and close buttons. Drag the empty space next to the tabs to move the window.", "window titlebar decorations frame close button minimize maximize chrome", false),
    item(S::Tabs, T::TabLayout, "Tab position", "Along the top, or a list down the side.", "vertical tabs sidebar horizontal", false),
    item(S::Tabs, T::TabShape, "Tab shape", "", "pill round boxy square sharp corners style", false),
    item(S::Tabs, T::TabWidth, "Tab width", "How wide tabs get when there's room.", "size narrow wide tabs", false),
    item(S::Tabs, T::TabCloseButtons, "Close buttons on tabs", "", "x cross close tab hover", false),
    item(S::Tabs, T::NewTabPosition, "Open new tabs next to the current tab", "Otherwise they open at the end.", "new tab position order", false),
    item(S::Tabs, T::LastTab, "Closing the last tab closes the window", "Otherwise a new tab opens.", "last tab quit window", false),
    item(S::Tabs, T::ForwardButton, "Forward button only when you can go forward", "", "toolbar forward hide", false),
    item(S::Tabs, T::ReloadButton, "Reload button", "", "toolbar refresh", false),
    item(S::Tabs, T::ReaderButton, "Reader mode button", "In the address bar.", "toolbar reader", false),
    item(S::Tabs, T::LoadingBar, "Loading bar", "The line under the toolbar while a page loads.", "progress loading indicator", false),
    item(S::Tabs, T::DomainOnly, "Show only the site name in the address bar", "The full address appears when you click it.", "url domain host simplified full address", false),
    item(S::Tabs, T::KeepClosedTab, "Instant reopen of closed tabs", "The last tab you closed stays loaded for two minutes, so Ctrl+Shift+T brings it back instantly, where you left it.", "reopen closed tab undo cache fast memory", false),
    item(S::Tabs, T::SleepTabs, "Sleeping tabs", "Unload tabs you haven't used for a while to save memory and battery. They reload when you select them. Pinned tabs and tabs playing sound stay awake.", "memory performance discard suspend battery", false),
    item(S::Tabs, T::BookmarksBar, "Bookmarks bar", "Show your bookmarks under the toolbar. Ctrl+Shift+B.", "favourites favorites", false),
    item(S::Tabs, T::HomeButton, "Home button", "", "toolbar", false),
    item(S::Tabs, T::BookmarkButton, "Bookmark star", "In the address bar.", "toolbar favourite", false),
    item(S::Tabs, T::DownloadsButton, "Downloads button", "Always shown while something is downloading.", "toolbar", false),
    item(S::StartPage, T::StartTitle, "Title", "The big heading on the start page.", "name heading logo", false),
    item(S::StartPage, T::StartBackground, "Background", "", "wallpaper gradient colour image photo", false),
    item(S::StartPage, T::Gradient, "Gradient", "Add up to six colours. Drag a dot to move where that colour comes from.", "gradient glow colour color intensity position spread fade", true),
    item(S::StartPage, T::StartColor, "Background colour", "", "wallpaper", false),
    item(S::StartPage, T::StartImage, "Background image", "PNG, JPEG, WebP, GIF or BMP. Large photos are scaled down.", "wallpaper photo picture", false),
    item(S::StartPage, T::StartVideo, "Video", "MP4, MOV or WebM. It plays muted and on repeat, and only while the new tab page is showing.", "video movie wallpaper animated background mp4 webm mov", true),
    item(S::StartPage, T::StartSearch, "Search box", "", "search field", false),
    item(S::StartPage, T::ClockFormat, "Clock format", "", "time 12 24 hour am pm", false),
    item(S::StartPage, T::StartTiles, "Number of tiles", "The most bookmark tiles and frequently visited tiles shown.", "shortcuts count how many", false),
    item(S::StartPage, T::StartClock, "Clock and date", "", "time", false),
    item(S::StartPage, T::StartBookmarks, "Bookmark tiles", "", "shortcuts", false),
    item(S::StartPage, T::StartFrequent, "Frequently visited tiles", "Sites you visit most, from your history.", "shortcuts top sites most visited", false),
    item(S::Search, T::SearchEngine, "Search engine", "Used when what you type in the address bar isn't an address.", "google duckduckgo bing brave startpage ecosia default", false),
    item(S::Search, T::CustomSearch, "Custom search address", "Put %s where the search words go, e.g. https://example.com/search?q=%s", "url template", true),
    item(S::Search, T::Suggestions, "Address bar suggestions", "Show matching bookmarks and history while you type.", "autocomplete dropdown", false),
    item(S::Startup, T::RestoreSession, "Reopen tabs from last time", "Restored tabs load when you select them.", "session restore startup", false),
    item(S::Startup, T::NewTabPage, "New tabs open", "", "new tab page blank home", false),
    item(S::Startup, T::HomePage, "Home page", "Opened by the Home button and at start-up. Leave empty for the start page.", "homepage url start", true),
    item(S::Downloads, T::DownloadFolder, "Download folder", "", "location save path directory", true),
    item(S::Downloads, T::AskWhereToSave, "Ask where to save each file", "", "save as dialog", false),
    item(S::Privacy, T::BlockTrackers, "Block ads and trackers", "Stops pages from loading known advertising and tracking servers run by other companies. Pages load faster and use less data.", "adblock ad blocker tracking protection privacy shield", false),
    item(S::Privacy, T::BlockerExceptions, "Sites where blocking is off", "Switch blocking off for a site from the padlock in the address bar.", "allowlist whitelist exceptions adblock", true),
    item(S::Privacy, T::SitePermissions, "Remembered permissions", "Answers you asked Ferrum to remember for camera, location, notifications and so on.", "site settings camera microphone location notifications allow block", true),
    item(S::Privacy, T::HttpsOnly, "HTTPS-only mode", "Only connect to sites over a secure connection. Sites without HTTPS won't load.", "secure ssl tls encryption http", false),
    item(S::Privacy, T::Notifications, "Notifications", "Let sites ask to show notifications. Off by default.", "permission alerts", false),
    item(S::Privacy, T::ClearHistory, "Clear history", "Remove every page from your history and start-page tiles.", "delete erase browsing data", false),
    item(S::Privacy, T::ClearSiteData, "Clear cookies and site data", "Signs you out of websites.", "delete erase storage cache localstorage", false),
    item(S::WebContent, T::DarkWebsites, "Dark mode for every website", "In dark mode, darken sites that only have a light look.", "dark night black invert theme force", false),
    item(S::WebContent, T::DefaultZoom, "Default page zoom", "Change one tab's zoom with Ctrl +/−.", "zoom size scale", false),
    item(S::Advanced, T::UserAgent, "User agent", "What Ferrum tells websites it is. Leave empty for the default.", "ua user-agent browser string spoof", true),
    item(S::Advanced, T::DataFolder, "Profile folder", "Settings, bookmarks, history, cookies and the crash log.", "data files location", false),
    item(S::Advanced, T::ImportBrowser, "Import from another browser", "Copy bookmarks and history from browsers on this computer. Nothing is changed in the other browser.", "import chrome edge firefox safari brave opera vivaldi arc migrate switch move", true),
    item(S::Advanced, T::BookmarkFile, "Import or export bookmarks", "Bookmark files (HTML) from Firefox, Chrome, Edge and Safari can be imported, and exported files open in any of them.", "import export html backup migrate move", false),
    item(S::Advanced, T::CheckUpdates, "Check for updates", "Look for a newer Ferrum on GitHub once a day.", "update upgrade new version release", false),
    item(S::Advanced, T::ResetAll, "Reset all settings", "Bookmarks, history and cookies are kept.", "defaults restore factory", false),
    item(S::About, T::About, "", "", "version licence license engine webkit webview2 github bug report", true),
];

const ACCENTS: [[u8; 3]; 9] = [
    DEFAULT_ACCENT,
    [214, 62, 62],
    [226, 160, 30],
    [64, 160, 90],
    [32, 150, 160],
    [52, 110, 220],
    [110, 80, 210],
    [196, 70, 160],
    [110, 115, 125],
];

/// An on/off switch.
fn toggle(ui: &mut Ui, on: &mut bool, accent: Color32) -> egui::Response {
    let size = vec2(38.0, 21.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let off = ui.visuals().widgets.inactive.bg_fill;
    let fill = mix(off, accent, t);
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    if response.hovered() {
        ui.painter().rect_stroke(rect, rect.height() / 2.0, Stroke::new(1.0_f32, accent.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    let r = rect.height() / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + r + 3.0)..=(rect.right() - r - 3.0), t);
    ui.painter().circle_filled(pos2(x, rect.center().y), r, Color32::WHITE);
    response
}

/// A row of mutually exclusive choices.
fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, choices: &[(T, &str)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (choice, label) in choices {
            ui.selectable_value(value, *choice, *label);
        }
    });
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke.color;
    Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(Stroke::new(1.0_f32, stroke))
        .corner_radius(ui.visuals().window_corner_radius)
        .inner_margin(Margin::symmetric(18, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn row(ui: &mut Ui, label: &str, desc: &str, wide: bool, control: impl FnOnce(&mut Ui)) {
    ui.add_space(10.0);
    if wide {
        if !label.is_empty() {
            ui.label(RichText::new(label).size(14.0).strong());
        }
        if !desc.is_empty() {
            ui.add(egui::Label::new(RichText::new(desc).size(12.0).weak()).wrap());
        }
        ui.add_space(6.0);
        control(ui);
    } else {
        ui.horizontal(|ui| {
            let text_width = (ui.available_width() * 0.58).max(180.0);
            ui.allocate_ui_with_layout(vec2(text_width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_max_width(text_width);
                ui.add(egui::Label::new(RichText::new(label).size(14.0).strong()).wrap());
                if !desc.is_empty() {
                    ui.add(egui::Label::new(RichText::new(desc).size(12.0).weak()).wrap());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), control);
        });
    }
    ui.add_space(10.0);
}

impl Browser {
    pub(crate) fn draw_settings_page(&mut self, ui: &mut Ui, rect: egui::Rect) {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
        let before = format!("{:?}", self.profile.settings);
        let accent = self.accent();

        let nav_width = if rect.width() > 760.0 { 230.0 } else { 170.0 };
        let nav = egui::Rect::from_min_size(rect.min, vec2(nav_width, rect.height()));
        let content = egui::Rect::from_min_max(pos2(nav.right(), rect.top()), rect.max);

        // ----- left: title, search, sections -----
        ui.scope_builder(egui::UiBuilder::new().max_rect(nav.shrink2(vec2(14.0, 18.0))), |ui| {
            ui.label(RichText::new("Settings").size(24.0).strong());
            ui.add_space(10.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.settings_query)
                    .id(Id::new("settings_search"))
                    .hint_text("Search settings")
                    .margin(Margin::symmetric(8, 5))
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(10.0);
            let searching = !self.settings_query.trim().is_empty();
            for section in Section::ALL {
                let selected = !searching && self.settings_section == section;
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                if selected {
                    ui.painter().rect_filled(r, 6.0, accent.gamma_multiply(0.18));
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min + vec2(0.0, 6.0), vec2(3.0, r.height() - 12.0)), 1.5, accent);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let color = if selected { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
                ui.painter().text(r.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, section.name(), FontId::proportional(14.0), color);
                if resp.clicked() {
                    self.settings_section = section;
                    self.settings_query.clear();
                }
            }
        });
        ui.painter().vline(nav.right(), rect.y_range(), Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color));

        // ----- right: the chosen section, or search results -----
        let query: Vec<String> = self.settings_query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        let matches = |i: &Item| {
            let hay = format!("{} {} {} {}", i.label, i.desc, i.keywords, i.section.name()).to_lowercase();
            query.iter().all(|w| hay.contains(w.as_str()))
        };
        let shown: Vec<&Item> = ITEMS
            .iter()
            .filter(|i| self.visible(i.setting))
            .filter(|i| if query.is_empty() { i.section == self.settings_section } else { matches(i) })
            .collect();
        let salt = if query.is_empty() { format!("{:?}", self.settings_section) } else { "search".into() };

        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            egui::ScrollArea::vertical().id_salt(("settings_scroll", salt)).auto_shrink(false).show(ui, |ui| {
                let width = (ui.available_width() - 48.0).min(760.0);
                let side = ((ui.available_width() - width) / 2.0).max(24.0);
                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        if shown.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| ui.label(RichText::new("No settings match your search").weak().size(15.0)));
                            return;
                        }
                        let mut current: Option<Section> = None;
                        let mut i = 0;
                        while i < shown.len() {
                            let section = shown[i].section;
                            let group: Vec<&Item> = shown[i..].iter().take_while(|it| it.section == section).copied().collect();
                            i += group.len();
                            if current != Some(section) {
                                current = Some(section);
                                ui.label(RichText::new(section.name()).size(20.0).strong());
                                if !section.blurb().is_empty() && query.is_empty() {
                                    ui.label(RichText::new(section.blurb()).weak());
                                }
                                ui.add_space(10.0);
                            }
                            card(ui, |ui| {
                                for (n, it) in group.iter().enumerate() {
                                    if n > 0 {
                                        ui.separator();
                                    }
                                    row(ui, it.label, it.desc, it.wide, |ui| self.setting_control(ui, it.setting));
                                }
                            });
                            ui.add_space(22.0);
                        }
                    });
                });
            });
        });

        if format!("{:?}", self.profile.settings) != before {
            self.profile.mark_dirty();
            self.apply_engine_settings();
        }
    }

    /// Settings that only make sense with another setting's value.
    fn visible(&self, setting: Setting) -> bool {
        let s = &self.profile.settings;
        match setting {
            Setting::CustomSearch => s.search_engine == SearchEngine::Custom,
            Setting::StartColor => s.start_background == StartBackground::Color,
            Setting::Gradient => s.start_background == StartBackground::Gradient,
            Setting::StartImage => s.start_background == StartBackground::Image,
            Setting::StartVideo => s.start_background == StartBackground::Video,
            _ => true,
        }
    }

    fn setting_control(&mut self, ui: &mut Ui, setting: Setting) {
        let accent = self.accent();
        let s = &mut self.profile.settings;
        match setting {
            Setting::Theme => self.theme_gallery(ui),
            Setting::Mode => {
                segmented(ui, &mut s.theme, &[(ThemeChoice::Dark, "Dark"), (ThemeChoice::Light, "Light"), (ThemeChoice::System, "System")]);
            },
            Setting::Accent => {
                ui.horizontal_wrapped(|ui| {
                    for preset in ACCENTS {
                        let color = Color32::from_rgb(preset[0], preset[1], preset[2]);
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 11.0, color);
                        if s.accent == preset {
                            ui.painter().circle_stroke(r.center(), 13.0, Stroke::new(2.0_f32, ui.visuals().strong_text_color()));
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            s.accent = preset;
                        }
                    }
                    ui.add_space(6.0);
                    let mut c = Color32::from_rgb(s.accent[0], s.accent[1], s.accent[2]);
                    if ui.color_edit_button_srgba(&mut c).on_hover_text("Any colour").changed() {
                        s.accent = [c.r(), c.g(), c.b()];
                    }
                    if let Some(p) = s.theme_preset.palette_for(ui.ctx().theme() == egui::Theme::Dark)
                        && s.accent != p.accent && ui.small_button("Theme's colour").clicked() {
                            s.accent = p.accent;
                        }
                });
            },
            Setting::UiScale => {
                ui.add(egui::Slider::new(&mut s.ui_scale, 0.8..=1.5).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            },
            Setting::Density => segmented(ui, &mut s.compact, &[(true, "Compact"), (false, "Comfortable")]),
            Setting::Roundness => {
                ui.add(egui::Slider::new(&mut s.roundness, 0..=20));
            },
            Setting::TabLayout => segmented(ui, &mut s.tab_layout, &[(TabLayout::Vertical, "Side"), (TabLayout::Top, "Top")]),
            Setting::BookmarksBar => {
                toggle(ui, &mut s.show_bookmarks_bar, accent);
            },
            Setting::HomeButton => {
                toggle(ui, &mut s.show_home_button, accent);
            },
            Setting::TabShape => segmented(
                ui,
                &mut s.tab_shape,
                &[(TabShape::Sharp, "Sharp"), (TabShape::Boxy, "Boxy"), (TabShape::Pill, "Pill"), (TabShape::Classic, "Classic")],
            ),
            Setting::TabWidth => segmented(ui, &mut s.tab_width, &[(TabWidth::Wide, "Wide"), (TabWidth::Normal, "Normal"), (TabWidth::Narrow, "Narrow")]),
            Setting::TabCloseButtons => segmented(
                ui,
                &mut s.tab_close_buttons,
                &[(CloseButtons::Active, "Current tab"), (CloseButtons::Hover, "On hover"), (CloseButtons::All, "All tabs")],
            ),
            Setting::KeepClosedTab => {
                toggle(ui, &mut s.keep_closed_tab, accent);
            },
            Setting::NewTabPosition => {
                toggle(ui, &mut s.new_tab_next_to_current, accent);
            },
            Setting::LastTab => {
                toggle(ui, &mut s.close_window_with_last_tab, accent);
            },
            Setting::ForwardButton => {
                toggle(ui, &mut s.forward_when_available, accent);
            },
            Setting::ReloadButton => {
                toggle(ui, &mut s.show_reload_button, accent);
            },
            Setting::ReaderButton => {
                toggle(ui, &mut s.show_reader_button, accent);
            },
            Setting::LoadingBar => {
                toggle(ui, &mut s.show_loading_bar, accent);
            },
            Setting::DomainOnly => {
                toggle(ui, &mut s.domain_only_address, accent);
            },
            Setting::Lite => {
                if toggle(ui, &mut s.lite, accent).changed() {
                    self.toast(if self.profile.settings.lite { "Lite mode on — new pages use less memory" } else { "Lite mode off" });
                }
            },
            Setting::EnergySaver => {
                ui.vertical(|ui| {
                    segmented(
                        ui,
                        &mut s.energy_saver,
                        &[(EnergySaver::Always, "Always"), (EnergySaver::Unplugged, "When unplugged"), (EnergySaver::LowBattery, "At 20% battery"), (EnergySaver::Off, "Off")],
                    );
                    if s.saving_energy {
                        ui.label(RichText::new("On now").color(Color32::from_rgb(52, 168, 83)));
                    }
                });
            },
            Setting::Animations => {
                if s.lite {
                    ui.label(RichText::new("Off in Lite mode").weak());
                } else {
                    toggle(ui, &mut s.animations, accent);
                }
            },
            Setting::StartSearch => {
                toggle(ui, &mut s.start_show_search, accent);
            },
            Setting::ClockFormat => segmented(ui, &mut s.clock_24h, &[(true, "24-hour"), (false, "12-hour")]),
            Setting::StartTiles => segmented(ui, &mut s.start_tiles, &[(32, "32"), (24, "24"), (16, "16"), (8, "8")]),
            Setting::BookmarkButton => {
                toggle(ui, &mut s.show_bookmark_button, accent);
            },
            Setting::DownloadsButton => {
                toggle(ui, &mut s.show_downloads_button, accent);
            },
            Setting::StartTitle => {
                ui.add(egui::TextEdit::singleline(&mut s.start_title).desired_width(200.0));
            },
            Setting::StartBackground => {
                egui::ComboBox::from_id_salt("start_bg")
                    .selected_text(match s.start_background {
                        StartBackground::Plain => "Plain",
                        StartBackground::Gradient => "Gradient",
                        StartBackground::Color => "Colour",
                        StartBackground::Image => "Image",
                        StartBackground::Video => "Video",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.start_background, StartBackground::Plain, "Plain");
                        ui.selectable_value(&mut s.start_background, StartBackground::Gradient, "Gradient");
                        ui.selectable_value(&mut s.start_background, StartBackground::Color, "Colour");
                        ui.selectable_value(&mut s.start_background, StartBackground::Image, "Image");
                        ui.selectable_value(&mut s.start_background, StartBackground::Video, "Video");
                    });
            },
            Setting::Gradient => {
                if s.gradient_sources.is_empty() {
                    s.gradient_sources.push(Default::default());
                }
                let g = crate::ui::Gradient::from_settings(s, accent);
                let n = s.gradient_sources.len();
                let mut sel = self.gradient_selected.min(n - 1);
                ui.horizontal(|ui| {
                    // A live preview: drag a dot to move that colour; click elsewhere to move the selected one.
                    let (rect, resp) = ui.allocate_exact_size(vec2(240.0, 150.0), Sense::click_and_drag());
                    g.paint(&ui.painter().with_clip_rect(rect), rect);
                    ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
                    let at = |pos: [f32; 2]| rect.min + vec2(pos[0], pos[1]) * rect.size();
                    let spots: Vec<egui::Pos2> = s.gradient_sources.iter().map(|g| at(g.pos)).collect();
                    let dot = |i: usize| spots[i];
                    if (resp.drag_started() || resp.clicked())
                        && let Some(p) = resp.interact_pointer_pos()
                            && let Some(i) = (0..n).filter(|&i| dot(i).distance(p) < 14.0).min_by(|&a, &b| dot(a).distance(p).total_cmp(&dot(b).distance(p))) {
                                sel = i;
                            }
                    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
                        let v = (p - rect.min) / rect.size();
                        s.gradient_sources[sel].pos = [v.x.clamp(0.0, 1.0), v.y.clamp(0.0, 1.0)];
                    }
                    for i in 0..n {
                        let c = g.sources[i].0;
                        let r = if i == sel { 8.0 } else { 6.0 };
                        let d = at(s.gradient_sources[i].pos);
                        ui.painter().circle_filled(d, r, c);
                        ui.painter().circle_stroke(d, r, Stroke::new(if i == sel { 3.0_f32 } else { 2.0 }, Color32::WHITE));
                    }
                    resp.on_hover_cursor(egui::CursorIcon::Crosshair);

                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        // Pick a colour to change; add or remove colours.
                        ui.horizontal(|ui| {
                            let mut remove = None;
                            for i in 0..n {
                                let c = g.sources[i].0;
                                let (r, chip) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                                ui.painter().circle_filled(r.center(), 10.0, c);
                                if i == sel {
                                    ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0_f32, ui.visuals().strong_text_color()));
                                }
                                let chip = chip.on_hover_text(format!("Colour {} — right-click to remove", i + 1));
                                if chip.clicked() {
                                    sel = i;
                                }
                                if chip.secondary_clicked() && n > 1 {
                                    remove = Some(i);
                                }
                            }
                            if n < crate::ui::Gradient::MAX_SOURCES && ui.small_button("+ Add colour").clicked() {
                                // Somewhere new, in a colour that differs from the last one.
                                let spots = [[1.0, 1.0], [0.0, 1.0], [1.0, 0.0], [0.0, 0.0], [0.5, 0.5], [0.5, 1.0]];
                                let last = g.sources.last().map_or(accent, |s| s.0);
                                let hsva = egui::ecolor::Hsva::from(last);
                                let next: Color32 = egui::ecolor::Hsva::new((hsva.h + 0.33).fract(), hsva.s.max(0.5), hsva.v.max(0.6), 1.0).into();
                                s.gradient_sources.push(crate::profile::GradientSource {
                                    color: Some([next.r(), next.g(), next.b()]),
                                    intensity: 0.8,
                                    spread: 1.0,
                                    pos: spots[n % spots.len()],
                                });
                                sel = n;
                            }
                            if let Some(i) = remove {
                                s.gradient_sources.remove(i);
                                sel = sel.min(s.gradient_sources.len() - 1);
                            }
                        });
                        ui.add_space(4.0);
                        let mut remove_selected = false;
                        let src = &mut s.gradient_sources[sel];
                        egui::Grid::new("gradient_grid").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                            ui.label(format!("Colour {}", sel + 1));
                            ui.horizontal(|ui| {
                                let mut c = src.color.map_or(accent, |[r, g, b]| Color32::from_rgb(r, g, b));
                                if ui.color_edit_button_srgba(&mut c).changed() {
                                    src.color = Some([c.r(), c.g(), c.b()]);
                                }
                                if src.color.is_some() {
                                    if ui.small_button("Use accent").clicked() {
                                        src.color = None;
                                    }
                                } else {
                                    ui.label(RichText::new("Accent colour").weak());
                                }
                            });
                            ui.end_row();
                            ui.label("Intensity");
                            ui.add(egui::Slider::new(&mut src.intensity, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                            ui.end_row();
                            ui.label("Spread");
                            ui.add(egui::Slider::new(&mut src.spread, 0.2..=3.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                            ui.end_row();
                            ui.label("Position");
                            ui.horizontal(|ui| {
                                for (label, pos) in [("Top", [0.5, 0.0]), ("Centre", [0.5, 0.5]), ("Bottom", [0.5, 1.0]), ("Left", [0.0, 0.5]), ("Right", [1.0, 0.5])] {
                                    if ui.small_button(label).clicked() {
                                        src.pos = pos;
                                    }
                                }
                            });
                            ui.end_row();
                            if n > 1 {
                                ui.label("");
                                remove_selected = ui.small_button(format!("Remove colour {}", sel + 1)).clicked();
                                ui.end_row();
                            }
                        });
                        if remove_selected {
                            s.gradient_sources.remove(sel);
                            sel = sel.min(s.gradient_sources.len() - 1);
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Fades into");
                            let mut c = g.base;
                            if ui.color_edit_button_srgba(&mut c).changed() {
                                s.gradient_base = Some([c.r(), c.g(), c.b()]);
                            }
                            if s.gradient_base.is_some() {
                                if ui.small_button("Automatic").clicked() {
                                    s.gradient_base = None;
                                }
                            } else {
                                ui.label(RichText::new("Automatic").weak());
                            }
                        });
                    });
                });
                self.gradient_selected = sel;
            },
            Setting::StartColor => {
                let mut c = Color32::from_rgb(s.start_color[0], s.start_color[1], s.start_color[2]);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    s.start_color = [c.r(), c.g(), c.b()];
                }
            },
            Setting::StartVideo => {
                let mut pick = false;
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        pick = ui.button("Choose video…").clicked();
                        if !s.start_video.is_empty() && ui.button("Remove").clicked() {
                            s.start_video.clear();
                        }
                        let name = std::path::Path::new(&s.start_video)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "No video chosen".into());
                        ui.label(RichText::new(name).weak());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Darken");
                        ui.add(egui::Slider::new(&mut s.video_dim, 0..=80).suffix("%"));
                    });
                    if s.lite {
                        ui.label(RichText::new("Lite mode is on, so the new tab page shows a still colour instead.").weak());
                    }
                });
                if pick {
                    self.choose_start_video();
                }
            },
            Setting::StartImage => {
                if !s.start_image.is_empty() && ui.button("Remove").clicked() {
                    s.start_image.clear();
                }
                if ui.button("Choose…").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() {
                        s.start_image = path.to_string_lossy().into_owned();
                    }
                let name = std::path::Path::new(&s.start_image)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "None".into());
                ui.label(RichText::new(name).weak());
            },
            Setting::StartClock => {
                toggle(ui, &mut s.start_show_clock, accent);
            },
            Setting::StartBookmarks => {
                toggle(ui, &mut s.start_show_bookmarks, accent);
            },
            Setting::StartFrequent => {
                toggle(ui, &mut s.start_show_frequent, accent);
            },
            Setting::SearchEngine => {
                egui::ComboBox::from_id_salt("engine").selected_text(s.search_engine.name()).show_ui(ui, |ui| {
                    for e in SearchEngine::ALL {
                        if ui.selectable_value(&mut s.search_engine, e, e.name()).clicked() {
                            s.search_engine_chosen = true;
                        }
                    }
                });
            },
            Setting::CustomSearch => {
                ui.add(egui::TextEdit::singleline(&mut s.custom_search_url).hint_text("https://example.com/search?q=%s").desired_width(f32::INFINITY));
                if !s.custom_search_url.contains("%s") {
                    ui.label(RichText::new("Needs %s — DuckDuckGo is used until then").size(12.0).color(Color32::from_rgb(210, 120, 40)));
                }
            },
            Setting::Suggestions => {
                toggle(ui, &mut s.show_suggestions, accent);
            },
            Setting::RestoreSession => {
                toggle(ui, &mut s.restore_session, accent);
            },
            Setting::NewTabPage => segmented(
                ui,
                &mut s.new_tab_page,
                &[(NewTabPage::Blank, "Blank page"), (NewTabPage::HomePage, "Home page"), (NewTabPage::StartPage, "Start page")],
            ),
            Setting::HomePage => {
                ui.add(egui::TextEdit::singleline(&mut s.home_page).hint_text("https://…").desired_width(f32::INFINITY));
            },
            Setting::DownloadFolder => {
                let current = if s.download_dir.trim().is_empty() {
                    dirs::download_dir().map(|d| d.display().to_string()).unwrap_or_else(|| "Downloads".into())
                } else {
                    s.download_dir.clone()
                };
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(current).monospace()).truncate());
                });
                ui.horizontal(|ui| {
                    if ui.button("Change…").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            s.download_dir = dir.to_string_lossy().into_owned();
                        }
                    if !s.download_dir.is_empty() && ui.button("Use the Downloads folder").clicked() {
                        s.download_dir.clear();
                    }
                });
            },
            Setting::AskWhereToSave => {
                toggle(ui, &mut s.ask_where_to_save, accent);
            },
            Setting::HttpsOnly => {
                toggle(ui, &mut s.https_only, accent);
            },
            Setting::Notifications => {
                toggle(ui, &mut s.allow_notifications, accent);
            },
            Setting::ClearHistory => {
                let private = self.profile.is_private();
                if ui.add_enabled(!private && !self.profile.history.is_empty(), egui::Button::new("Clear")).clicked() {
                    self.profile.clear_history();
                    self.toast("History cleared");
                }
            },
            Setting::ClearSiteData => {
                if ui.button("Clear").clicked() {
                    self.clear_site_data();
                }
            },
            Setting::DarkWebsites => {
                toggle(ui, &mut s.force_dark_websites, accent);
            },
            Setting::DefaultZoom => {
                ui.add(egui::Slider::new(&mut s.default_zoom, 0.5..=2.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            },
            Setting::UserAgent => {
                ui.add(egui::TextEdit::singleline(&mut s.user_agent).hint_text("Default").desired_width(f32::INFINITY));
                if !s.user_agent.is_empty() && ui.small_button("Use the default").clicked() {
                    s.user_agent.clear();
                }
            },
            Setting::DataFolder => {
                match self.profile.dir().map(|d| d.to_path_buf()) {
                    Some(dir) => {
                        if ui.button("Open").on_hover_text(dir.display().to_string()).clicked() {
                            crate::downloads::open_path(&dir);
                        }
                    },
                    None => {
                        ui.label(RichText::new("None in a private window").weak());
                    },
                }
            },
            Setting::ResetAll => {
                // Ask for a second click within a few seconds, so it's never accidental.
                let id = Id::new("reset_all_armed");
                let now = ui.input(|i| i.time);
                let armed_at: Option<f64> = ui.ctx().data(|d| d.get_temp(id));
                let armed = armed_at.is_some_and(|t| now - t < 4.0);
                let label = if armed { "Click again to reset" } else { "Reset" };
                let button = if armed {
                    egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(Color32::from_rgb(200, 60, 50))
                } else {
                    egui::Button::new(label)
                };
                if ui.add(button).clicked() {
                    if armed {
                        ui.ctx().data_mut(|d| d.remove::<f64>(id));
                        self.profile.settings = Settings::default();
                        self.toast("All settings reset");
                    } else {
                        ui.ctx().data_mut(|d| d.insert_temp(id, now));
                    }
                }
                if armed {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                }
            },
            Setting::About => self.about(ui),
            Setting::BlockTrackers => {
                toggle(ui, &mut s.block_trackers, accent);
                ui.label(RichText::new(format!("{} servers listed", self.blocker.domain_count())).weak().size(11.0));
            },
            Setting::BlockerExceptions => {
                if s.blocker_exceptions.is_empty() {
                    ui.label(RichText::new("None").weak());
                }
                let mut remove = None;
                for (i, host) in s.blocker_exceptions.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(host);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    s.blocker_exceptions.remove(i);
                }
            },
            Setting::SitePermissions => {
                if s.site_permissions.is_empty() {
                    ui.label(RichText::new("None").weak());
                }
                let mut forget: Option<String> = None;
                if !s.site_permissions.is_empty() {
                    egui::Grid::new("site_permissions").num_columns(3).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                        for (host, features) in &s.site_permissions {
                            ui.label(host);
                            let list: Vec<String> = features.iter().map(|(f, ok)| format!("{f}: {}", if *ok { "allowed" } else { "blocked" })).collect();
                            ui.label(RichText::new(list.join(", ")).weak());
                            if ui.small_button("Forget").clicked() {
                                forget = Some(host.clone());
                            }
                            ui.end_row();
                        }
                    });
                }
                if let Some(host) = forget {
                    s.site_permissions.remove(&host);
                }
            },
            Setting::SleepTabs => {
                toggle(ui, &mut s.sleep_tabs, accent);
                if s.sleep_tabs {
                    ui.add(egui::DragValue::new(&mut s.sleep_minutes).range(5..=480).prefix("after ").suffix(" min"));
                }
            },
            Setting::ImportBrowser => {
                if self.import_sources.is_none() {
                    self.import_sources = Some(crate::browser_import::find_sources());
                }
                let sources = self.import_sources.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.import_choice.0, "Bookmarks");
                    ui.checkbox(&mut self.import_choice.1, "History");
                });
                ui.add_space(4.0);
                let mut chosen = None;
                if sources.is_empty() {
                    ui.label(RichText::new("No other browsers found on this computer.").weak());
                }
                let (any, _) = (self.import_choice.0 || self.import_choice.1, ());
                for source in &sources {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&source.name).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(any, egui::Button::new("Import")).clicked() {
                                chosen = Some(source.clone());
                            }
                        });
                    });
                }
                if ui.small_button("Look again").clicked() {
                    self.import_sources = None;
                }
                if let Some(source) = chosen {
                    self.import_from_browser(&source);
                }
            },
            Setting::BookmarkFile => {
                ui.horizontal(|ui| {
                    if ui.button("Import…").clicked() {
                        self.import_bookmarks();
                    }
                    if ui.button("Export…").clicked() {
                        self.export_bookmarks();
                    }
                });
            },
            Setting::TitleBar => {
                let before = s.custom_title_bar;
                toggle(ui, &mut s.custom_title_bar, accent);
                let now = s.custom_title_bar;
                if cfg!(target_os = "macos") {
                    if now != self.custom_title_bar_at_start {
                        ui.label(RichText::new("Restart Ferrum to apply").weak().size(11.0));
                    }
                } else if now != before {
                    self.window.set_decorations(!now);
                    self.custom_title_bar_at_start = now;
                }
            },
            Setting::CheckUpdates => {
                toggle(ui, &mut s.check_updates, accent);
            },
        }
    }

    fn theme_gallery(&mut self, ui: &mut Ui) {
        // Preview Ferrum in its own light/dark choice, not the current theme's mode.
        let ferrum_dark = match self.profile.settings.theme {
            ThemeChoice::Dark => true,
            ThemeChoice::Light => false,
            ThemeChoice::System => ui.ctx().system_theme() == Some(egui::Theme::Dark),
        };
        let current = self.profile.settings.theme_preset;
        let accent = self.accent();
        let card_size = vec2(124.0, 104.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            for preset in ThemePreset::ALL {
                let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let (panel, surface, text) = preset.preview(ferrum_dark);
                // The accent of the version shown.
                let accent_shown = preset.palette_for(ferrum_dark).map(|p| p.accent);
                let [r, g, b] = accent_shown.unwrap_or(DEFAULT_ACCENT);
                let dot = Color32::from_rgb(r, g, b);
                let p = ui.painter();
                let preview = egui::Rect::from_min_size(rect.min, vec2(rect.width(), 72.0));
                p.rect_filled(preview, 8.0, panel);
                // A tiny browser: tab, toolbar, two lines of text and an accent button.
                let toolbar = egui::Rect::from_min_size(preview.min + vec2(0.0, 16.0), vec2(preview.width(), 14.0));
                p.rect_filled(egui::Rect::from_min_size(preview.min + vec2(8.0, 6.0), vec2(34.0, 10.0)), CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, surface);
                p.rect_filled(toolbar, 0.0, surface);
                p.rect_filled(egui::Rect::from_min_size(toolbar.min + vec2(24.0, 3.5), vec2(70.0, 7.0)), 3.5, mix(surface, text, 0.12));
                for (n, w) in [(0, 64.0), (1, 44.0)] {
                    let y = toolbar.bottom() + 10.0 + n as f32 * 10.0;
                    p.rect_filled(egui::Rect::from_min_size(pos2(preview.left() + 10.0, y), vec2(w, 5.0)), 2.5, mix(panel, text, 0.55));
                }
                p.rect_filled(egui::Rect::from_min_size(pos2(preview.right() - 34.0, preview.bottom() - 18.0), vec2(24.0, 10.0)), 5.0, dot);
                let selected = preset == current;
                let stroke = if selected {
                    Stroke::new(2.5_f32, accent)
                } else if resp.hovered() {
                    Stroke::new(1.5_f32, ui.visuals().widgets.hovered.bg_stroke.color)
                } else {
                    Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color)
                };
                p.rect_stroke(preview, 8.0, stroke, StrokeKind::Inside);
                let name_color = if selected { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
                p.text(pos2(rect.left() + 2.0, preview.bottom() + 8.0), egui::Align2::LEFT_TOP, preset.name(), FontId::proportional(13.0), name_color);
                if resp.clicked() {
                    let s = &mut self.profile.settings;
                    s.theme_preset = preset;
                    s.accent = accent_shown.unwrap_or(DEFAULT_ACCENT);
                    // Google's look is pill-shaped all over.
                    if preset == ThemePreset::Google {
                        s.tab_shape = crate::profile::TabShape::Pill;
                        s.roundness = s.roundness.max(16);
                    }
                }
            }
        });
    }

    fn about(&mut self, ui: &mut Ui) {
        let id = Id::new("about_icon");
        let texture: Option<egui::TextureHandle> = ui.ctx().data(|d| d.get_temp(id));
        let texture = texture.or_else(|| {
            let img = image::load_from_memory(include_bytes!("../assets/icon-256.png")).ok()?.to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let tex = ui.ctx().load_texture("about-icon", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(id, tex.clone()));
            Some(tex)
        });
        let mut open: Option<&str> = None;
        ui.horizontal(|ui| {
            if let Some(tex) = &texture {
                ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::splat(72.0)));
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(APP_NAME).size(22.0).strong());
                ui.label(format!("Version {}", crate::VERSION));
                let mut check = false;
                ui.horizontal(|ui| match self.update_status() {
                    None => check = ui.small_button("Check for updates").clicked(),
                    Some(UpdateStatus::Checking) => {
                        ui.spinner();
                        ui.label("Checking for updates…");
                    },
                    Some(UpdateStatus::UpToDate) => {
                        ui.label(RichText::new("Ferrum is up to date").weak());
                        check = ui.small_button("Check again").clicked();
                    },
                    Some(UpdateStatus::Available { version, asset, .. }) => {
                        ui.label(RichText::new(format!("Ferrum {version} is available")).strong().color(self.accent()));
                        let label = if asset.is_some() { "Update and restart" } else { "Download" };
                        if ui.button(label).clicked() {
                            self.start_update();
                        }
                    },
                    Some(UpdateStatus::Downloading { version, received, total }) => {
                        let fraction = total.filter(|t| *t > 0).map(|t| received as f32 / t as f32);
                        let text = format!("Downloading Ferrum {version}… {}", crate::downloads::human_size(received));
                        match fraction {
                            Some(f) => ui.add(egui::ProgressBar::new(f).desired_width(260.0).text(text)),
                            None => ui.add(egui::ProgressBar::new(0.0).animate(true).desired_width(260.0).text(text)),
                        };
                    },
                    Some(UpdateStatus::Installing { version }) | Some(UpdateStatus::Installed { version, .. }) => {
                        ui.spinner();
                        ui.label(format!("Installing Ferrum {version}… Ferrum will restart with your tabs."));
                    },
                    Some(UpdateStatus::Failed(e)) => {
                        ui.label(RichText::new("Couldn't update").weak()).on_hover_text(&e);
                        ui.label(RichText::new(e).weak().size(11.0));
                        check = ui.small_button("Try again").clicked();
                    },
                });
                if check {
                    self.check_for_updates();
                }
                ui.label(RichText::new(format!("{} engine · MIT licence", ENGINE_NAME)).weak());
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Source code").clicked() {
                        open = Some("https://github.com/p1e2r/ferrum");
                    }
                    if ui.button("Release notes").clicked() {
                        open = Some("https://github.com/p1e2r/ferrum/releases");
                    }
                    if ui.button("Report a problem").clicked() {
                        open = Some("https://github.com/p1e2r/ferrum/issues/new");
                    }
                    let log = crate::crash_log_path();
                    if log.exists() && ui.button("Last crash report").clicked() {
                        crate::downloads::open_path(&log);
                    }
                });
            });
        });
        ui.add_space(4.0);
        if let Some(url) = open.and_then(|u| Url::parse(u).ok()) {
            self.open_tab(Some(url));
        }
    }
}

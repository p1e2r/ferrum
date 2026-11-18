// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.app.DownloadManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material.icons.filled.Bookmarks
import androidx.compose.material.icons.filled.Calculate
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.NorthWest
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.SystemUpdate
import androidx.compose.material.icons.filled.FindInPage
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView

/** Focus for the address bar, so the start page's search box can hand over to it. */
val addressFocus = FocusRequester()

@Composable
fun FerrumApp(activity: MainActivity) {
    val browser = activity.browser
    val focus = LocalFocusManager.current

    BackHandler {
        when {
            browser.fullscreenView != null -> browser.exitFullscreen()
            browser.switcherOpen -> browser.switcherOpen = false
            browser.editing -> { browser.editing = false; focus.clearFocus() }
            browser.screen != Screen.Browser -> browser.screen = Screen.Browser
            browser.findOpen -> browser.stopFind()
            browser.back() -> {}
            else -> activity.moveTaskToBack(true)
        }
    }

    // Test tags double as Android view ids, so the emulator test can find the address bar.
    @OptIn(ExperimentalComposeUiApi::class)
    val root = Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }
    Surface(color = MaterialTheme.colorScheme.background, modifier = root) {
        val full = browser.fullscreenView
        if (full != null) {
            AndroidView(
                factory = { ctx -> FrameLayout(ctx).apply { setBackgroundColor(android.graphics.Color.BLACK) } },
                modifier = Modifier.fillMaxSize(),
                update = { frame ->
                    if (frame.getChildAt(0) !== full) {
                        frame.removeAllViews()
                        (full.parent as? ViewGroup)?.removeView(full)
                        frame.addView(full, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    }
                },
            )
            return@Surface
        }
        Box(Modifier.fillMaxSize().systemBarsPadding().imePadding()) {
            when (browser.screen) {
                Screen.Settings -> SettingsScreen(activity)
                Screen.Bookmarks -> ListScreen(activity, bookmarks = true)
                Screen.History -> ListScreen(activity, bookmarks = false)
                Screen.Browser -> BrowserView(activity)
            }
            AnimatedVisibility(browser.switcherOpen, enter = fadeIn(), exit = fadeOut()) { TabSwitcher(browser) }
        }
        browser.siteRequest?.let { req ->
            AlertDialog(
                onDismissRequest = { req.answer(false) },
                title = { Text("Allow ${req.what}?") },
                text = { Text("${req.origin} wants to use your ${req.what}.") },
                confirmButton = { TextButton(onClick = { req.answer(true) }) { Text("Allow") } },
                dismissButton = { TextButton(onClick = { req.answer(false) }) { Text("Block") } },
            )
        }
    }
}

@Composable
private fun BrowserView(activity: MainActivity) {
    val browser = activity.browser
    val tab = browser.active
    Column(Modifier.fillMaxSize()) {
        TopBar(activity, browser.addressText, onText = { browser.addressText = it })
        Box(Modifier.fillMaxWidth().height(2.dp)) {
            if (tab.loading && !tab.isStart) {
                LinearProgressIndicator(
                    progress = { tab.progress / 100f },
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.primary,
                    trackColor = Color.Transparent,
                    drawStopIndicator = {},
                )
            }
        }
        if (browser.findOpen) FindBar(browser)
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (tab.isStart) StartPage(activity) else PageView(browser, tab)
            if (browser.editing) Suggestions(activity, browser.addressText.text, onFill = { browser.addressText = TextFieldValue(it, TextRange(it.length)) }) { browser.editing = false }
        }
    }
}

/** The tab's WebView, swapped in when the tab changes or its view is remade. */
@Composable
private fun PageView(browser: Browser, tab: Tab) {
    AndroidView(
        factory = { ctx -> FrameLayout(ctx) },
        modifier = Modifier.fillMaxSize().testTag("page"),
        update = { frame ->
            tab.viewVersion // redo this when the view is replaced
            val wv = browser.webViewFor(tab)
            if (frame.childCount != 1 || frame.getChildAt(0) !== wv) {
                frame.removeAllViews()
                (wv.parent as? ViewGroup)?.removeView(wv)
                frame.addView(wv, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            }
        },
    )
}

@Composable
private fun TopBar(activity: MainActivity, text: TextFieldValue, onText: (TextFieldValue) -> Unit) {
    val browser = activity.browser
    val settings = activity.settings
    val tab = browser.active
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val colors = MaterialTheme.colorScheme
    var hadFocus by remember { mutableStateOf(false) }
    LaunchedEffect(browser.editing) {
        if (browser.editing) runCatching { addressFocus.requestFocus() } else {
            focus.clearFocus()
            keyboard?.hide()
            // Focus goes to the page, not the next button: a keyboard's Enter is released
            // after the address is submitted, and would otherwise press that button.
            browser.active.webView?.requestFocus()
        }
    }

    fun submit(value: String) {
        val answer = Calc.answer(value)
        if (answer != null) {
            copy(activity, answer)
            browser.toast("Copied $answer")
            return
        }
        browser.go(value)
        browser.editing = false
    }

    Row(
        Modifier.fillMaxWidth().height(60.dp).padding(start = 10.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).height(44.dp).clip(CircleShape).background(colors.surfaceVariant)
                .then(if (browser.editing) Modifier.border(1.5.dp, colors.primary, CircleShape) else Modifier)
                .padding(start = 14.dp, end = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val secure = tab.url.startsWith("https://")
            Icon(
                when {
                    browser.editing || tab.isStart -> Icons.Filled.Search
                    secure -> Icons.Filled.Lock
                    else -> Icons.Filled.Info
                },
                contentDescription = if (secure) "Secure connection" else null,
                tint = colors.onSurfaceVariant,
                modifier = Modifier.size(18.dp),
            )
            Spacer(Modifier.width(10.dp))
            Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                BasicTextField(
                    value = if (browser.editing) text else TextFieldValue(shortAddress(tab, settings)),
                    onValueChange = { if (browser.editing) onText(it) },
                    singleLine = true,
                    textStyle = TextStyle(color = colors.onSurface, fontSize = 16.sp),
                    cursorBrush = SolidColor(colors.primary),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                    keyboardActions = KeyboardActions(onGo = { submit(text.text) }),
                    // Focusable only while editing: otherwise Android would hand it focus on its
                    // own (with a keyboard, or after a page loads) and the bar would never let go.
                    modifier = Modifier.fillMaxWidth().focusRequester(addressFocus).focusProperties { canFocus = browser.editing }
                        .testTag("address-field")
                        .onPreviewKeyEvent { e ->
                            // A keyboard's Enter: handle both press and release here, so the
                            // release doesn't land on whatever gets focus next.
                            if (e.key == Key.Enter || e.key == Key.NumPadEnter) {
                                if (e.type == KeyEventType.KeyDown) submit(text.text)
                                true
                            } else false
                        }
                        .onFocusChanged { f ->
                            if (f.isFocused) hadFocus = true
                            else if (hadFocus) { hadFocus = false; browser.editing = false }
                        },
                )
                if (!browser.editing) {
                    // Tapping the address starts editing it.
                    Box(Modifier.matchParentSize().clickable(interactionSource = null, indication = null) { browser.startEditing() }.testTag("address"))
                }
                if (!browser.editing && tab.isStart) {
                    Text("Search or type address", color = colors.onSurfaceVariant, fontSize = 16.sp, maxLines = 1)
                }
            }
            if (browser.editing && text.text.isNotEmpty()) {
                IconButton(onClick = { onText(TextFieldValue("")) }, modifier = Modifier.size(36.dp)) {
                    Icon(Icons.Filled.Close, "Clear", tint = colors.onSurfaceVariant, modifier = Modifier.size(18.dp))
                }
            } else if (!browser.editing && !tab.isStart && settings.blockAds) {
                val host = tab.pageHost
                val allowed = host != null && host in settings.adsAllowed
                Row(
                    Modifier.clip(CircleShape).clickable {
                        if (host != null) {
                            settings.setAdsAllowed(host, !allowed)
                            browser.toast(if (allowed) "Blocking ads on $host" else "Ads allowed on $host")
                            browser.reload()
                        }
                    }.padding(horizontal = 8.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Filled.Shield, "Ad blocking", tint = if (allowed) colors.onSurfaceVariant.copy(alpha = 0.4f) else colors.primary, modifier = Modifier.size(16.dp))
                    if (tab.blocked > 0 && !allowed) {
                        Spacer(Modifier.width(3.dp))
                        Text("${tab.blocked}", fontSize = 12.sp, color = colors.onSurfaceVariant)
                    }
                }
            }
        }
        TabCount(browser)
        Menu(activity)
    }
}

private fun shortAddress(tab: Tab, settings: Settings): String {
    if (tab.isStart) return ""
    val shown = Location.display(tab.url, settings.searchEngine)
    if (shown != tab.url) return shown
    return tab.url.removePrefix("https://").removePrefix("http://").removePrefix("www.").trimEnd('/')
}

fun copy(context: Context, text: String) {
    (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Ferrum", text))
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TabCount(browser: Browser) {
    val colors = MaterialTheme.colorScheme
    Box(
        Modifier.size(44.dp).clip(CircleShape).focusProperties { canFocus = false }.combinedClickable(
            onClick = { browser.editing = false; browser.captureThumbnail(browser.active); browser.switcherOpen = true },
            onLongClick = { browser.newTab() },
            onClickLabel = "Show tabs",
        ).testTag("tabs"),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier.size(22.dp).border(1.8.dp, colors.onSurface, RoundedCornerShape(6.dp)),
            contentAlignment = Alignment.Center,
        ) {
            val n = browser.tabs.size
            Text(if (n > 99) ":D" else "$n", fontSize = if (n > 9) 10.sp else 12.sp, fontWeight = FontWeight.Bold, color = colors.onSurface)
        }
    }
}

@Composable
private fun Menu(activity: MainActivity) {
    val browser = activity.browser
    val settings = activity.settings
    val store = activity.store
    val updater = activity.updater
    val tab = browser.active
    var open by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    val update = updater.state as? Updater.State.Available

    Box {
        // The toolbar buttons never take keyboard focus: a keyboard's Enter, released after an
        // address is submitted, would otherwise press whichever one Android focused next.
        IconButton(onClick = { open = true }, modifier = Modifier.focusProperties { canFocus = false }.testTag("menu")) {
            Box {
                Icon(Icons.Filled.MoreVert, "Menu", tint = colors.onSurface)
                if (update != null) Box(Modifier.size(8.dp).clip(CircleShape).background(colors.primary).align(Alignment.TopEnd))
            }
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, modifier = Modifier.width(240.dp)) {
            fun act(f: () -> Unit) { open = false; f() }
            Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                IconButton(onClick = { act { browser.back() } }, enabled = tab.canGoBack || !tab.isStart) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back")
                }
                IconButton(onClick = { act { browser.forward() } }, enabled = tab.canGoForward) {
                    Icon(Icons.AutoMirrored.Filled.ArrowForward, "Forward")
                }
                val marked = !tab.isStart && store.isBookmarked(tab.url)
                IconButton(onClick = {
                    act {
                        val added = store.toggleBookmark(tab.url, tab.title)
                        browser.toast(if (added) "Bookmarked" else "Bookmark removed")
                    }
                }, enabled = !tab.isStart) {
                    Icon(if (marked) Icons.Filled.Bookmark else Icons.Filled.BookmarkBorder, "Bookmark", tint = if (marked) colors.primary else colors.onSurface)
                }
                IconButton(onClick = { act { browser.reload() } }, enabled = !tab.isStart) {
                    Icon(if (tab.loading && !tab.isStart) Icons.Filled.Close else Icons.Filled.Refresh, "Reload")
                }
            }
            HorizontalDivider(color = colors.outlineVariant)
            if (update != null) {
                MenuRow("Update to Ferrum ${update.version}", Icons.Filled.SystemUpdate, tint = colors.primary) {
                    act { updater.download(); browser.toast("Downloading Ferrum ${update.version}…") }
                }
            }
            MenuRow("New tab", Icons.Filled.Add) { act { browser.newTab() } }
            MenuRow("Home", Icons.Filled.Home) { act { browser.goHome() } }
            MenuRow("Bookmarks", Icons.Filled.Bookmarks) { act { browser.screen = Screen.Bookmarks } }
            MenuRow("History", Icons.Filled.History) { act { browser.screen = Screen.History } }
            MenuRow("Downloads", Icons.Filled.Download) {
                act { runCatching { activity.startActivity(Intent(DownloadManager.ACTION_VIEW_DOWNLOADS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
            }
            if (!tab.isStart) {
                MenuRow("Find in page", Icons.Filled.FindInPage) { act { browser.findOpen = true } }
                MenuRow("Share", Icons.Filled.Share) { act { browser.share(tab.url, tab.title) } }
            }
            DropdownMenuItem(
                text = { Text("Desktop site") },
                leadingIcon = { Icon(Icons.Filled.Computer, null) },
                trailingIcon = { Checkbox(checked = settings.desktopSite, onCheckedChange = null) },
                onClick = { act { browser.setDesktopSite(!settings.desktopSite) } },
            )
            MenuRow("Settings", Icons.Filled.Settings) { act { browser.screen = Screen.Settings } }
        }
    }
}

@Composable
private fun MenuRow(label: String, icon: ImageVector, tint: Color = Color.Unspecified, onClick: () -> Unit) {
    DropdownMenuItem(
        text = { Text(label, color = if (tint == Color.Unspecified) MaterialTheme.colorScheme.onSurface else tint) },
        leadingIcon = { Icon(icon, null, tint = if (tint == Color.Unspecified) MaterialTheme.colorScheme.onSurfaceVariant else tint) },
        onClick = onClick,
    )
}

@Composable
private fun FindBar(browser: Browser) {
    val colors = MaterialTheme.colorScheme
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 4.dp).height(44.dp).clip(CircleShape).background(colors.surfaceVariant).padding(start = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicTextField(
            value = browser.findQuery,
            onValueChange = { browser.find(it) },
            singleLine = true,
            textStyle = TextStyle(color = colors.onSurface, fontSize = 15.sp),
            cursorBrush = SolidColor(colors.primary),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { browser.findNext(true) }),
            modifier = Modifier.weight(1f).focusRequester(focus),
            decorationBox = { inner ->
                if (browser.findQuery.isEmpty()) Text("Find in page", color = colors.onSurfaceVariant, fontSize = 15.sp)
                inner()
            },
        )
        if (browser.findQuery.isNotEmpty()) {
            Text("${browser.findIndex}/${browser.findMatches}", color = colors.onSurfaceVariant, fontSize = 13.sp)
        }
        IconButton(onClick = { browser.findNext(false) }) { Icon(Icons.Filled.KeyboardArrowUp, "Previous") }
        IconButton(onClick = { browser.findNext(true) }) { Icon(Icons.Filled.KeyboardArrowDown, "Next") }
        IconButton(onClick = { browser.stopFind() }) { Icon(Icons.Filled.Close, "Close find") }
    }
}

/** What's typed, as a sum, an address, a search, and matching bookmarks and history. */
@Composable
private fun Suggestions(activity: MainActivity, text: String, onFill: (String) -> Unit, done: () -> Unit) {
    val browser = activity.browser
    val settings = activity.settings
    val colors = MaterialTheme.colorScheme
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val t = text.trim()

    fun open(url: String) {
        browser.load(url)
        done()
        focus.clearFocus()
        keyboard?.hide()
    }

    Surface(color = colors.background, modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize().padding(horizontal = 8.dp)) {
            val answer = Calc.answer(t)
            if (answer != null) {
                item {
                    SuggestionRow(Icons.Filled.Calculate, "= $answer", "Tap to copy", highlight = true) {
                        copy(activity, answer)
                        browser.toast("Copied $answer")
                    }
                }
            }
            if (t.isNotEmpty() && Location.looksLikeAddress(t)) {
                item { SuggestionRow(Icons.Filled.Language, t, "Go to address") { open(Location.toUrl(t, settings.searchEngine)) } }
            }
            if (t.isNotEmpty()) {
                item { SuggestionRow(Icons.Filled.Search, t, "Search ${settings.searchEngine.label}") { open(settings.searchEngine.searchUrl(t)) } }
            }
            val matches = if (t.isEmpty()) {
                activity.store.history.asReversed().take(8).map { it.title to it.url }
            } else activity.store.suggestions(t, 8)
            if (t.isEmpty() && matches.isNotEmpty()) {
                item { Text("Recent", color = colors.onSurfaceVariant, fontSize = 13.sp, modifier = Modifier.padding(start = 14.dp, top = 12.dp, bottom = 4.dp)) }
            }
            items(matches, key = { it.second }) { (title, url) ->
                SuggestionRow(
                    if (activity.store.isBookmarked(url)) Icons.Filled.Bookmark else Icons.Filled.History,
                    title.ifBlank { url },
                    url.removePrefix("https://").removePrefix("http://").removePrefix("www."),
                    fill = { onFill(url) },
                ) { open(url) }
            }
        }
    }
}

@Composable
private fun SuggestionRow(
    icon: ImageVector,
    title: String,
    subtitle: String,
    highlight: Boolean = false,
    fill: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).clickable(onClick = onClick)
            .then(if (highlight) Modifier.background(colors.primary.copy(alpha = 0.12f)) else Modifier)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (highlight) colors.primary else colors.onSurfaceVariant, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = colors.onSurface, fontSize = if (highlight) 20.sp else 15.sp, fontWeight = if (highlight) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = colors.onSurfaceVariant, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (fill != null) {
            IconButton(onClick = fill, modifier = Modifier.size(36.dp)) {
                Icon(Icons.Filled.NorthWest, "Edit this address", tint = colors.onSurfaceVariant.copy(alpha = 0.6f), modifier = Modifier.size(16.dp))
            }
        }
    }
}

/** A tab's or site's little picture: its icon if it has one, otherwise its first letter. */
@Composable
fun SiteIcon(title: String, url: String, favicon: android.graphics.Bitmap?, size: Int = 20) {
    if (favicon != null) {
        Image(favicon.asImageBitmap(), null, Modifier.size(size.dp).clip(RoundedCornerShape((size / 4).dp)))
    } else {
        val label = (title.ifBlank { Location.host(url) }).firstOrNull { it.isLetterOrDigit() }?.uppercase() ?: "•"
        Box(Modifier.size(size.dp).clip(CircleShape).background(tileColor(Location.host(url))), contentAlignment = Alignment.Center) {
            Text(label, color = Color.White, fontSize = (size * 0.5f).sp, fontWeight = FontWeight.Bold)
        }
    }
}

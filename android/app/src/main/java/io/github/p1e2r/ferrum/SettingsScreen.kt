// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

@Composable
fun ScreenHeader(title: String, onBack: () -> Unit, actions: @Composable () -> Unit = {}) {
    Row(Modifier.fillMaxWidth().height(60.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
        Text(title, fontSize = 20.sp, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.onBackground, modifier = Modifier.weight(1f))
        actions()
    }
}

@Composable
fun SettingsScreen(activity: MainActivity) {
    val s = activity.settings
    val browser = activity.browser
    val updater = activity.updater
    val colors = MaterialTheme.colorScheme
    var clearing by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Settings", onBack = { browser.screen = Screen.Browser })
        LazyColumn(Modifier.fillMaxSize().padding(horizontal = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item {
                Section("Search") {
                    Choice("Search engine", s.searchEngine.label, SearchEngine.entries.map { it.label }) { i -> s.update(searchEngine = SearchEngine.entries[i]) }
                }
            }
            item {
                Section("Appearance") {
                    Choice("Theme", s.themeMode.label, ThemeMode.entries.map { it.label }) { i ->
                        s.update(themeMode = ThemeMode.entries[i])
                        activity.applyNightMode()
                    }
                    Text("Colours", color = colors.onSurface, fontSize = 15.sp, modifier = Modifier.padding(top = 10.dp, bottom = 8.dp))
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        items(Palette.entries) { p ->
                            val chosen = p == s.palette
                            Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.clip(RoundedCornerShape(14.dp)).clickable { s.update(palette = p) }.padding(6.dp)) {
                                Box(
                                    Modifier.size(40.dp).clip(CircleShape).background(Color(p.accent))
                                        .border(if (chosen) 3.dp else 0.dp, if (chosen) colors.onSurface else Color.Transparent, CircleShape),
                                    contentAlignment = Alignment.Center,
                                ) { if (chosen) Icon(Icons.Filled.Check, null, tint = Color.White, modifier = Modifier.size(20.dp)) }
                                Text(p.label, fontSize = 12.sp, color = colors.onSurfaceVariant, modifier = Modifier.padding(top = 4.dp))
                            }
                        }
                    }
                    Toggle("24-hour clock", null, s.clock24h) { s.update(clock24h = it) }
                    TextSetting("Start page title", s.startTitle) { s.update(startTitle = it) }
                }
            }
            item {
                Section("New tabs") {
                    Choice("New tabs show", s.newTabPage.label, NewTabPage.entries.map { it.label }) { i -> s.update(newTabPage = NewTabPage.entries[i]) }
                    TextSetting("Home page", s.homePage, placeholder = "The start page", keyboard = KeyboardType.Uri) { s.update(homePage = it) }
                    Toggle("Reopen tabs", "Open the tabs you had when Ferrum starts", s.restoreTabs) { s.update(restoreTabs = it) }
                }
            }
            item {
                Section("Pages") {
                    Toggle("Block ads and trackers", "${AdBlock.domainCount()} blocked sites", s.blockAds) { s.update(blockAds = it) }
                    Toggle("Dark websites", "Darken pages that have no dark look of their own", s.darkWebsites) { s.update(darkWebsites = it); browser.applySettings() }
                    Toggle("Desktop site", "Ask for sites' computer layouts", s.desktopSite) { browser.setDesktopSite(it) }
                    Toggle("JavaScript", null, s.javascript) { s.update(javascript = it); browser.applySettings() }
                    var zoom by remember { mutableFloatStateOf(s.textZoom.toFloat()) }
                    Text("Text size: ${zoom.roundToInt()}%", color = colors.onSurface, fontSize = 15.sp, modifier = Modifier.padding(top = 10.dp))
                    Slider(
                        value = zoom, onValueChange = { zoom = it }, valueRange = 50f..200f, steps = 14,
                        onValueChangeFinished = { s.update(textZoom = (zoom / 10).roundToInt() * 10); browser.applySettings() },
                    )
                }
            }
            item {
                Section("Privacy") {
                    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable { clearing = true }.padding(vertical = 12.dp)) {
                        Column {
                            Text("Clear browsing data", color = colors.onSurface, fontSize = 15.sp)
                            Text("History, cookies, cached pages", color = colors.onSurfaceVariant, fontSize = 13.sp)
                        }
                    }
                }
            }
            item {
                Section("About") {
                    Text("Ferrum ${BuildConfig.VERSION_NAME} for Android", color = colors.onSurface, fontSize = 15.sp)
                    Text("Free software under the MIT licence. Pages are shown by Android System WebView.", color = colors.onSurfaceVariant, fontSize = 13.sp, modifier = Modifier.padding(top = 2.dp, bottom = 10.dp))
                    val st = updater.state
                    val status = when (st) {
                        Updater.State.Idle -> null
                        Updater.State.Checking -> "Checking…"
                        Updater.State.UpToDate -> "Ferrum is up to date"
                        is Updater.State.Available -> "Ferrum ${st.version} is available"
                        is Updater.State.Downloading -> "Downloading ${st.version}… ${st.percent}%"
                        is Updater.State.Ready -> "Ferrum ${st.version} is ready to install"
                        is Updater.State.Failed -> st.message
                    }
                    status?.let { Text(it, color = colors.onSurfaceVariant, fontSize = 13.sp, modifier = Modifier.padding(bottom = 8.dp)) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        when (st) {
                            is Updater.State.Available -> Button(onClick = { updater.download() }) { Text("Update") }
                            is Updater.State.Ready -> Button(onClick = { updater.install() }) { Text("Install") }
                            is Updater.State.Checking, is Updater.State.Downloading -> {}
                            else -> Button(onClick = { s.lastUpdateCheck = System.currentTimeMillis(); updater.check() }) { Text("Check for updates") }
                        }
                        TextButton(onClick = { browser.screen = Screen.Browser; browser.newTab("https://github.com/${Updater.REPO}") }) { Text("Source code") }
                    }
                    Toggle("Check for updates automatically", null, s.checkUpdates) { s.update(checkUpdates = it) }
                }
            }
            item { Spacer(Modifier.height(24.dp)) }
        }
    }

    if (clearing) {
        var history by remember { mutableStateOf(true) }
        var cookies by remember { mutableStateOf(false) }
        var cache by remember { mutableStateOf(true) }
        AlertDialog(
            onDismissRequest = { clearing = false },
            title = { Text("Clear browsing data") },
            text = {
                Column {
                    CheckRow("History", history) { history = it }
                    CheckRow("Cookies and site data (signs you out)", cookies) { cookies = it }
                    CheckRow("Cached pages and images", cache) { cache = it }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    browser.clearData(cache = cache, cookies = cookies, history = history)
                    clearing = false
                    browser.toast("Cleared")
                }) { Text("Clear") }
            },
            dismissButton = { TextButton(onClick = { clearing = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(colors.surface).border(1.dp, colors.outlineVariant, RoundedCornerShape(24.dp)).padding(16.dp)) {
        Text(title, color = colors.primary, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(bottom = 6.dp))
        content()
    }
}

@Composable
private fun Toggle(label: String, detail: String?, value: Boolean, onChange: (Boolean) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable { onChange(!value) }.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(label, color = colors.onSurface, fontSize = 15.sp)
            detail?.let { Text(it, color = colors.onSurfaceVariant, fontSize = 13.sp) }
        }
        Spacer(Modifier.width(12.dp))
        Switch(checked = value, onCheckedChange = onChange)
    }
}

@Composable
private fun Choice(label: String, value: String, options: List<String>, onPick: (Int) -> Unit) {
    val colors = MaterialTheme.colorScheme
    var open by remember { mutableStateOf(false) }
    Box {
        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable { open = true }.padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(label, color = colors.onSurface, fontSize = 15.sp, modifier = Modifier.weight(1f))
            Text(value, color = colors.primary, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEachIndexed { i, o ->
                DropdownMenuItem(
                    text = { Text(o) },
                    trailingIcon = { if (o == value) Icon(Icons.Filled.Check, null, tint = colors.primary) },
                    onClick = { open = false; onPick(i) },
                )
            }
        }
    }
}

@Composable
private fun TextSetting(label: String, value: String, placeholder: String? = null, keyboard: KeyboardType = KeyboardType.Text, onDone: (String) -> Unit) {
    var text by remember(value) { mutableStateOf(value) }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it; onDone(it.trim()) },
        label = { Text(label) },
        placeholder = { if (placeholder != null) Text(placeholder) },
        singleLine = true,
        shape = RoundedCornerShape(16.dp),
        keyboardOptions = KeyboardOptions(keyboardType = keyboard, imeAction = ImeAction.Done),
        modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
    )
}

@Composable
private fun CheckRow(label: String, value: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().clickable { onChange(!value) }, verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked = value, onCheckedChange = onChange)
        Text(label)
    }
}

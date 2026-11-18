// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.scale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** What a new tab shows: the clock, a search box, and bookmarks and favourite sites. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun StartPage(activity: MainActivity) {
    val settings = activity.settings
    val store = activity.store
    val browser = activity.browser
    val colors = MaterialTheme.colorScheme
    val accent = colors.primary

    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            now = System.currentTimeMillis()
            delay(1000 - now % 1000)
        }
    }
    val time = SimpleDateFormat(if (settings.clock24h) "HH:mm" else "h:mm", Locale.getDefault()).format(Date(now))
    val date = SimpleDateFormat("EEEE d MMMM", Locale.getDefault()).format(Date(now))

    val tiles = store.bookmarks.map { Triple(it.title, it.url, true) } +
        store.frequent(12).map { Triple(it.title, it.url, false) }

    Box(
        Modifier.fillMaxSize().background(
            Brush.radialGradient(
                listOf(accent.copy(alpha = 0.32f), accent.copy(alpha = 0.10f), Color.Transparent),
                radius = 1400f,
                center = androidx.compose.ui.geometry.Offset(0f, 0f),
            ),
        ).background(Brush.verticalGradient(listOf(Color.Transparent, colors.background.copy(alpha = 0.6f)))),
    ) {
        LazyVerticalGrid(
            columns = GridCells.Adaptive(78.dp),
            modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp).testTag("start"),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(top = 48.dp, bottom = 20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(time, fontSize = 64.sp, fontWeight = FontWeight.Light, color = colors.onBackground)
                    Text(date, fontSize = 15.sp, color = colors.onSurfaceVariant)
                    Spacer(Modifier.height(26.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(34.dp).clip(RoundedCornerShape(10.dp))) {
                            // The launcher icon's two layers (the adaptive icon itself can't be drawn here).
                            Image(painterResource(R.mipmap.ic_launcher_background), null, Modifier.fillMaxSize())
                            Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.fillMaxSize().scale(1.45f))
                        }
                        Spacer(Modifier.size(10.dp))
                        Text(settings.startTitle.ifBlank { "Ferrum" }, fontSize = 30.sp, fontWeight = FontWeight.Bold, color = colors.onBackground)
                    }
                    Spacer(Modifier.height(22.dp))
                    Row(
                        Modifier.widthIn(max = 560.dp).fillMaxWidth().height(52.dp).clip(CircleShape)
                            .background(colors.surface).border(1.dp, colors.outlineVariant, CircleShape)
                            .clickable { browser.startEditing() }.testTag("start-search").padding(horizontal = 18.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Icons.Filled.Search, null, tint = colors.onSurfaceVariant, modifier = Modifier.size(20.dp))
                        Spacer(Modifier.size(12.dp))
                        Text("Search ${settings.searchEngine.label} or type an address", color = colors.onSurfaceVariant, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            items(tiles.take(16), key = { it.second }) { (title, url, bookmarked) ->
                var menu by remember { mutableStateOf(false) }
                Box {
                    Column(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp))
                            .combinedClickable(onClick = { browser.load(url) }, onLongClick = { menu = true })
                            .padding(vertical = 8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Box(
                            Modifier.size(54.dp).clip(RoundedCornerShape(18.dp)).background(colors.surface).border(1.dp, colors.outlineVariant, RoundedCornerShape(18.dp)),
                            contentAlignment = Alignment.Center,
                        ) { SiteIcon(title, url, null, size = 30) }
                        Spacer(Modifier.height(6.dp))
                        Text(
                            title.ifBlank { Location.host(url) }, fontSize = 12.sp, color = colors.onBackground, maxLines = 1,
                            overflow = TextOverflow.Ellipsis, textAlign = TextAlign.Center, modifier = Modifier.padding(horizontal = 2.dp),
                        )
                    }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(text = { Text("Open in new tab") }, onClick = { menu = false; browser.newTab(url, background = true, after = browser.activeIndex) })
                        DropdownMenuItem(
                            text = { Text(if (bookmarked) "Remove bookmark" else "Remove from start page") },
                            onClick = { menu = false; if (bookmarked) store.removeBookmark(url) else store.removeVisit(url) },
                        )
                    }
                }
            }
        }
    }
}


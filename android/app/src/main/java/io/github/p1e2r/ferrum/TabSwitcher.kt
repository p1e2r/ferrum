// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** All the tabs as cards: tap one to show it, ✕ to close it. */
@Composable
fun TabSwitcher(browser: Browser) {
    val colors = MaterialTheme.colorScheme
    val grid = rememberLazyGridState()
    LaunchedEffect(Unit) { grid.scrollToItem(browser.activeIndex.coerceAtLeast(0)) }

    Surface(color = colors.background, modifier = Modifier.fillMaxSize()) {
        Box(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize()) {
                Row(Modifier.fillMaxWidth().height(60.dp).padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (browser.tabs.size == 1) "1 tab" else "${browser.tabs.size} tabs",
                        fontSize = 20.sp, fontWeight = FontWeight.SemiBold, color = colors.onBackground,
                        modifier = Modifier.weight(1f).padding(start = 6.dp),
                    )
                    TextButton(onClick = { browser.closeAll(); browser.switcherOpen = false }) { Text("Close all") }
                    TextButton(onClick = { browser.switcherOpen = false }) { Text("Done", fontWeight = FontWeight.SemiBold) }
                }
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(160.dp),
                    state = grid,
                    modifier = Modifier.fillMaxSize().padding(horizontal = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    itemsIndexed(browser.tabs, key = { _, t -> t.id }) { i, tab ->
                        TabCard(tab, active = i == browser.activeIndex, onOpen = { browser.select(i); browser.switcherOpen = false }, onClose = { browser.close(tab) })
                    }
                }
            }
            FloatingActionButton(
                onClick = { browser.newTab(); browser.switcherOpen = false },
                containerColor = colors.primary,
                contentColor = colors.onPrimary,
                shape = RoundedCornerShape(20.dp),
                modifier = Modifier.align(Alignment.BottomEnd).padding(20.dp),
            ) { Icon(Icons.Filled.Add, "New tab") }
        }
    }
}

@Composable
private fun TabCard(tab: Tab, active: Boolean, onOpen: () -> Unit, onClose: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val shape = RoundedCornerShape(20.dp)
    Column(
        Modifier.fillMaxWidth().clip(shape).background(colors.surface)
            .border(if (active) 2.5.dp else 1.dp, if (active) colors.primary else colors.outlineVariant, shape)
            .clickable(onClick = onOpen),
    ) {
        Row(Modifier.fillMaxWidth().padding(start = 10.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            SiteIcon(tab.title, tab.url, tab.favicon, size = 16)
            Spacer(Modifier.width(8.dp))
            Text(
                if (tab.isStart) "New tab" else tab.title.ifBlank { Location.host(tab.url) },
                fontSize = 13.sp, color = colors.onSurface, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            IconButton(onClick = onClose, modifier = Modifier.size(36.dp)) {
                Icon(Icons.Filled.Close, "Close tab", modifier = Modifier.size(18.dp), tint = colors.onSurfaceVariant)
            }
        }
        Box(Modifier.fillMaxWidth().aspectRatio(0.8f).padding(start = 6.dp, end = 6.dp, bottom = 6.dp).clip(RoundedCornerShape(14.dp))) {
            val thumb = tab.thumbnail
            when {
                tab.isStart -> Box(
                    Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(colors.primary.copy(alpha = 0.35f), colors.surfaceVariant))),
                    contentAlignment = Alignment.Center,
                ) { Text("Ferrum", fontWeight = FontWeight.Bold, fontSize = 18.sp, color = colors.onSurface) }
                thumb != null -> Image(thumb.asImageBitmap(), null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop, alignment = Alignment.TopCenter)
                else -> Box(Modifier.fillMaxSize().background(colors.surfaceVariant), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(44.dp).clip(CircleShape), contentAlignment = Alignment.Center) { SiteIcon(tab.title, tab.url, tab.favicon, size = 40) }
                }
            }
        }
    }
}

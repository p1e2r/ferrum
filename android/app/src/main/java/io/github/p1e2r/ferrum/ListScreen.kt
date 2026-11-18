// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.text.DateFormat
import java.util.Date

/** Bookmarks or history, searchable, with a delete button on every row. */
@Composable
fun ListScreen(activity: MainActivity, bookmarks: Boolean) {
    val browser = activity.browser
    val store = activity.store
    val colors = MaterialTheme.colorScheme
    var query by remember { mutableStateOf("") }
    var confirmClear by remember { mutableStateOf(false) }

    val rows: List<Triple<String, String, String>> = if (bookmarks) {
        store.bookmarks.reversed().map { Triple(it.title, it.url, "") }
    } else {
        val fmt = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
        store.history.asReversed().map { Triple(it.title, it.url, fmt.format(Date(it.lastVisit))) }
    }
    val q = query.trim().lowercase()
    val shown = if (q.isEmpty()) rows else rows.filter { it.first.lowercase().contains(q) || it.second.lowercase().contains(q) }

    Column(Modifier.fillMaxSize()) {
        ScreenHeader(if (bookmarks) "Bookmarks" else "History", onBack = { browser.screen = Screen.Browser }) {
            if (!bookmarks && store.history.isNotEmpty()) TextButton(onClick = { confirmClear = true }) { Text("Clear all") }
        }
        Row(
            Modifier.padding(horizontal = 14.dp).fillMaxWidth().height(46.dp).clip(CircleShape).background(colors.surfaceVariant).padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Filled.Search, null, tint = colors.onSurfaceVariant)
            Spacer(Modifier.width(10.dp))
            BasicTextField(
                value = query, onValueChange = { query = it }, singleLine = true,
                textStyle = TextStyle(color = colors.onSurface, fontSize = 15.sp), cursorBrush = SolidColor(colors.primary),
                modifier = Modifier.weight(1f),
                decorationBox = { inner ->
                    if (query.isEmpty()) Text(if (bookmarks) "Search bookmarks" else "Search history", color = colors.onSurfaceVariant, fontSize = 15.sp)
                    inner()
                },
            )
        }
        if (shown.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.TopCenter) {
                Text(
                    when {
                        q.isNotEmpty() -> "Nothing matches"
                        bookmarks -> "No bookmarks yet. Tap ☆ in the ⋮ menu to add one."
                        else -> "No history yet"
                    },
                    color = colors.onSurfaceVariant,
                )
            }
        }
        LazyColumn(Modifier.fillMaxSize().padding(horizontal = 8.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            items(shown, key = { it.second + it.third }) { (title, url, detail) ->
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).clickable {
                        browser.screen = Screen.Browser
                        browser.load(url)
                    }.padding(start = 12.dp, top = 8.dp, bottom = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    SiteIcon(title, url, null, size = 32)
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(title.ifBlank { url }, color = colors.onSurface, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            url.removePrefix("https://").removePrefix("http://").removePrefix("www.") + if (detail.isNotEmpty()) " · $detail" else "",
                            color = colors.onSurfaceVariant, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        )
                    }
                    IconButton(onClick = { if (bookmarks) store.removeBookmark(url) else store.removeVisit(url) }) {
                        Icon(Icons.Filled.Delete, if (bookmarks) "Remove bookmark" else "Remove from history", tint = colors.onSurfaceVariant)
                    }
                }
            }
        }
    }

    if (confirmClear) {
        AlertDialog(
            onDismissRequest = { confirmClear = false },
            title = { Text("Clear all history?") },
            confirmButton = { TextButton(onClick = { store.clearHistory(); confirmClear = false }) { Text("Clear") } },
            dismissButton = { TextButton(onClick = { confirmClear = false }) { Text("Cancel") } },
        )
    }
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.content.Context
import android.content.SharedPreferences
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Light, dark, or as the phone is set. */
enum class ThemeMode(val label: String) { System("System"), Light("Light"), Dark("Dark") }

/** Ferrum's colour themes (a subset of the desktop's), each with its accent. */
enum class Palette(val label: String, val accent: Long, val darkAccent: Long) {
    Ferrum("Ferrum", 0xFFD9622B, 0xFFF08A5D),
    Google("Google", 0xFF1A73E8, 0xFF8AB4F8),
    Midnight("Midnight", 0xFF3464E6, 0xFF7C9CFF),
    Forest("Forest", 0xFF248F52, 0xFF5CC98A),
    Plum("Plum", 0xFF9646B4, 0xFFC98BE0),
    Rose("Rose", 0xFFD6466E, 0xFFF06E96),
}

/** What new tabs show. */
enum class NewTabPage(val label: String) { Start("Start page"), Home("Home page") }

/**
 * Settings, kept in SharedPreferences. Each one is Compose state, so changing
 * it redraws whatever shows it, and is saved at once.
 */
class Settings(context: Context) {
    private val p: SharedPreferences = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    private fun <E : Enum<E>> enum(key: String, values: Array<E>, default: E): E =
        values.firstOrNull { it.name == p.getString(key, null) } ?: default

    var searchEngine by mutableStateOf(enum("search", SearchEngine.entries.toTypedArray(), SearchEngine.DuckDuckGo))
        private set
    var themeMode by mutableStateOf(enum("theme", ThemeMode.entries.toTypedArray(), ThemeMode.System))
        private set
    var palette by mutableStateOf(enum("palette", Palette.entries.toTypedArray(), Palette.Ferrum))
        private set
    var blockAds by mutableStateOf(p.getBoolean("blockAds", true))
        private set
    var darkWebsites by mutableStateOf(p.getBoolean("darkWebsites", false))
        private set
    var desktopSite by mutableStateOf(p.getBoolean("desktopSite", false))
        private set
    var textZoom by mutableIntStateOf(p.getInt("textZoom", 100))
        private set
    var homePage by mutableStateOf(p.getString("homePage", "") ?: "")
        private set
    var newTabPage by mutableStateOf(enum("newTab", NewTabPage.entries.toTypedArray(), NewTabPage.Start))
        private set
    var clock24h by mutableStateOf(p.getBoolean("clock24h", true))
        private set
    var startTitle by mutableStateOf(p.getString("startTitle", "Ferrum") ?: "Ferrum")
        private set
    var restoreTabs by mutableStateOf(p.getBoolean("restoreTabs", true))
        private set
    var checkUpdates by mutableStateOf(p.getBoolean("checkUpdates", true))
        private set
    var javascript by mutableStateOf(p.getBoolean("javascript", true))
        private set
    var lastUpdateCheck: Long
        get() = p.getLong("lastUpdateCheck", 0)
        set(v) = p.edit().putLong("lastUpdateCheck", v).apply()
    /** Sites where ad blocking is switched off. */
    var adsAllowed by mutableStateOf(p.getStringSet("adsAllowed", emptySet())?.toSet() ?: emptySet())
        private set

    fun update(searchEngine: SearchEngine? = null, themeMode: ThemeMode? = null, palette: Palette? = null, blockAds: Boolean? = null,
               darkWebsites: Boolean? = null, desktopSite: Boolean? = null, textZoom: Int? = null, homePage: String? = null,
               newTabPage: NewTabPage? = null, clock24h: Boolean? = null, startTitle: String? = null, restoreTabs: Boolean? = null,
               checkUpdates: Boolean? = null, javascript: Boolean? = null) {
        val e = p.edit()
        searchEngine?.let { this.searchEngine = it; e.putString("search", it.name) }
        themeMode?.let { this.themeMode = it; e.putString("theme", it.name) }
        palette?.let { this.palette = it; e.putString("palette", it.name) }
        blockAds?.let { this.blockAds = it; e.putBoolean("blockAds", it) }
        darkWebsites?.let { this.darkWebsites = it; e.putBoolean("darkWebsites", it) }
        desktopSite?.let { this.desktopSite = it; e.putBoolean("desktopSite", it) }
        textZoom?.let { this.textZoom = it; e.putInt("textZoom", it) }
        homePage?.let { this.homePage = it; e.putString("homePage", it) }
        newTabPage?.let { this.newTabPage = it; e.putString("newTab", it.name) }
        clock24h?.let { this.clock24h = it; e.putBoolean("clock24h", it) }
        startTitle?.let { this.startTitle = it; e.putString("startTitle", it) }
        restoreTabs?.let { this.restoreTabs = it; e.putBoolean("restoreTabs", it) }
        checkUpdates?.let { this.checkUpdates = it; e.putBoolean("checkUpdates", it) }
        javascript?.let { this.javascript = it; e.putBoolean("javascript", it) }
        e.apply()
    }

    fun setAdsAllowed(host: String, allowed: Boolean) {
        adsAllowed = if (allowed) adsAllowed + host else adsAllowed - host
        p.edit().putStringSet("adsAllowed", adsAllowed).apply()
    }
}

data class Bookmark(val title: String, val url: String)

data class Visit(val url: String, val title: String, val visits: Int, val lastVisit: Long)

/** Bookmarks, history and the open tabs, as JSON files in the app's storage. */
class Store(private val context: Context) {
    val bookmarks = mutableStateListOf<Bookmark>()
    val history = mutableStateListOf<Visit>()

    private fun file(name: String) = File(context.filesDir, name)

    init {
        runCatching {
            val a = JSONArray(file("bookmarks.json").readText())
            for (i in 0 until a.length()) a.getJSONObject(i).let { bookmarks.add(Bookmark(it.optString("title"), it.getString("url"))) }
        }
        runCatching {
            val a = JSONArray(file("history.json").readText())
            for (i in 0 until a.length()) a.getJSONObject(i).let {
                history.add(Visit(it.getString("url"), it.optString("title"), it.optInt("visits", 1), it.optLong("lastVisit")))
            }
        }
    }

    private fun write(name: String, json: String) {
        val tmp = file("$name.tmp")
        tmp.writeText(json)
        tmp.renameTo(file(name))
    }

    fun saveBookmarks() = write("bookmarks.json", JSONArray(bookmarks.map { JSONObject().put("title", it.title).put("url", it.url) }).toString())

    private fun saveHistory() = write(
        "history.json",
        JSONArray(history.map { JSONObject().put("url", it.url).put("title", it.title).put("visits", it.visits).put("lastVisit", it.lastVisit) }).toString(),
    )

    fun isBookmarked(url: String) = bookmarks.any { it.url == url }

    fun toggleBookmark(url: String, title: String): Boolean {
        val i = bookmarks.indexOfFirst { it.url == url }
        val added = i < 0
        if (added) bookmarks.add(Bookmark(title.ifBlank { url }, url)) else bookmarks.removeAt(i)
        saveBookmarks()
        return added
    }

    fun removeBookmark(url: String) {
        bookmarks.removeAll { it.url == url }
        saveBookmarks()
    }

    fun recordVisit(url: String, title: String) {
        if (!url.startsWith("http")) return
        val now = System.currentTimeMillis()
        val i = history.indexOfFirst { it.url == url }
        if (i >= 0) {
            val v = history.removeAt(i)
            history.add(v.copy(title = title.ifBlank { v.title }, visits = v.visits + 1, lastVisit = now))
        } else {
            history.add(Visit(url, title, 1, now))
        }
        while (history.size > 5000) history.removeAt(0)
        saveHistory()
    }

    fun updateTitle(url: String, title: String) {
        val i = history.indexOfFirst { it.url == url }
        if (i >= 0 && title.isNotBlank() && history[i].title != title) {
            history[i] = history[i].copy(title = title)
            saveHistory()
        }
    }

    fun removeVisit(url: String) {
        history.removeAll { it.url == url }
        saveHistory()
    }

    fun clearHistory() {
        history.clear()
        saveHistory()
    }

    /** Bookmarks then history pages matching `text`, best first. */
    fun suggestions(text: String, max: Int): List<Pair<String, String>> {
        val q = text.trim().lowercase()
        if (q.isEmpty()) return emptyList()
        val out = LinkedHashMap<String, String>()
        bookmarks.filter { it.title.lowercase().contains(q) || it.url.lowercase().contains(q) }.forEach { out[it.url] = it.title }
        history.asReversed()
            .filter { it.title.lowercase().contains(q) || it.url.lowercase().removePrefix("https://").removePrefix("http://").removePrefix("www.").startsWith(q) || it.url.lowercase().contains(q) }
            .sortedByDescending { it.visits }
            .forEach { out.putIfAbsent(it.url, it.title) }
        return out.entries.take(max).map { it.value to it.key }
    }

    /** The most visited pages that aren't bookmarked, for the start page. */
    fun frequent(max: Int): List<Visit> =
        history.filter { !isBookmarked(it.url) }.sortedWith(compareByDescending<Visit> { it.visits }.thenByDescending { it.lastVisit }).take(max)

    fun saveSession(urls: List<String>, titles: List<String>, active: Int) {
        val o = JSONObject().put("tabs", JSONArray(urls)).put("titles", JSONArray(titles)).put("active", active)
        write("session.json", o.toString())
    }

    fun loadSession(): Triple<List<String>, List<String>, Int>? = runCatching {
        val o = JSONObject(file("session.json").readText())
        val tabs = o.getJSONArray("tabs")
        val titles = o.optJSONArray("titles") ?: JSONArray()
        Triple((0 until tabs.length()).map { tabs.getString(it) }, (0 until tabs.length()).map { titles.optString(it, "") }, o.optInt("active"))
    }.getOrNull()
}

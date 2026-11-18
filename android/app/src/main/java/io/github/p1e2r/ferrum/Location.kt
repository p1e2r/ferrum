// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import java.net.URLEncoder

/** The search engines Ferrum offers, as on the desktop. */
enum class SearchEngine(val label: String, private val base: String, private val param: String = "q") {
    DuckDuckGo("DuckDuckGo", "https://duckduckgo.com/"),
    Google("Google", "https://www.google.com/search"),
    Bing("Bing", "https://www.bing.com/search"),
    Brave("Brave Search", "https://search.brave.com/search"),
    Startpage("Startpage", "https://www.startpage.com/do/search", "query"),
    Ecosia("Ecosia", "https://www.ecosia.org/search");

    fun searchUrl(query: String): String = "$base?$param=" + URLEncoder.encode(query, "UTF-8")

    /** The site's host, to recognise its result pages. */
    val host: String get() = base.removePrefix("https://").substringBefore('/')
}

/** Address-bar text → an address to load, as the desktop app does (src/location.rs). */
object Location {
    private val SCHEMES = listOf("http://", "https://", "file://", "about:", "data:", "javascript:", "view-source:")
    private val TLD = Regex("""^[a-z0-9-]+(\.[a-z0-9-]+)*\.[a-z]{2,}$""")
    private val IPV4 = Regex("""^\d{1,3}(\.\d{1,3}){3}$""")

    fun looksLikeAddress(text: String): Boolean {
        val t = text.trim()
        if (t.isEmpty() || t.contains(' ')) return false
        val lower = t.lowercase()
        if (SCHEMES.any { lower.startsWith(it) }) return true
        val hostPort = lower.substringBefore('/').substringBefore('?').substringBefore('#')
        val host = hostPort.substringBefore(':')
        val port = hostPort.substringAfter(':', "")
        if (port.isNotEmpty() && port.any { !it.isDigit() }) return false
        return host == "localhost" || IPV4.matches(host) || TLD.matches(host)
    }

    /** An address to load: the text itself if it's an address, otherwise a search. */
    fun toUrl(text: String, engine: SearchEngine): String {
        val t = text.trim()
        if (looksLikeAddress(t)) {
            val lower = t.lowercase()
            if (SCHEMES.any { lower.startsWith(it) }) return t
            val local = lower.startsWith("localhost") || IPV4.matches(lower.substringBefore('/').substringBefore(':'))
            val withScheme = (if (local) "http://" else "https://") + t
            // A bare host gets a trailing slash, as browsers show it.
            return if (withScheme.substringAfter("://").contains('/')) withScheme else "$withScheme/"
        }
        return engine.searchUrl(t)
    }

    /** What the address bar shows when it isn't being edited: the address, or the search words. */
    fun display(url: String, engine: SearchEngine): String {
        if (url.isEmpty()) return ""
        val u = runCatching { java.net.URI(url) }.getOrNull() ?: return url
        if (u.host == engine.host && (u.path == "/search" || u.path == "/" || u.path == "/do/search")) {
            val q = u.rawQuery?.split('&')?.firstOrNull { it.startsWith("q=") || it.startsWith("query=") }?.substringAfter('=')
            if (q != null) return java.net.URLDecoder.decode(q, "UTF-8")
        }
        return url
    }

    /** "github.com" from "https://www.github.com/x". */
    fun host(url: String): String = runCatching { java.net.URI(url).host?.removePrefix("www.") }.getOrNull() ?: url
}

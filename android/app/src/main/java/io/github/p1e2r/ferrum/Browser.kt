// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.Manifest
import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.Message
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.GeolocationPermissions
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature
import java.io.ByteArrayInputStream

/** One tab. Its WebView is made when the tab is first shown, and dropped when it sleeps. */
class Tab(url: String = "", title: String = "") {
    val id = nextId++
    var url by mutableStateOf(url)
    var title by mutableStateOf(title)
    var progress by mutableIntStateOf(100)
    var favicon by mutableStateOf<Bitmap?>(null)
    var thumbnail by mutableStateOf<Bitmap?>(null)
    var canGoBack by mutableStateOf(false)
    var canGoForward by mutableStateOf(false)
    var blocked by mutableIntStateOf(0)
    /** Bumped when the WebView is replaced, so the screen swaps it in. */
    var viewVersion by mutableIntStateOf(0)
    var webView: WebView? = null

    /** The page's site, read by the ad blocker on WebView's own threads. */
    @Volatile var pageHost: String? = null

    val isStart get() = url.isEmpty()
    val loading get() = progress < 100

    companion object { private var nextId = 1 }
}

/** A site asking for the camera, microphone or location, waiting for an answer. */
data class SiteRequest(val origin: String, val what: String, val answer: (Boolean) -> Unit)

enum class Screen { Browser, Settings, Bookmarks, History }

/**
 * The browser: tabs, the WebViews in them, and what the screen is showing.
 * Everything here is Compose state, so the UI follows it.
 */
class Browser(val activity: MainActivity, val settings: Settings, val store: Store) {
    val tabs = mutableStateListOf<Tab>()
    var activeIndex by mutableIntStateOf(0)
    val active: Tab get() = tabs[activeIndex.coerceIn(0, tabs.lastIndex)]

    var screen by mutableStateOf(Screen.Browser)
    var switcherOpen by mutableStateOf(false)
    var editing by mutableStateOf(false)
    /** When an address was last submitted: a keyboard's Enter is released just after, and mustn't press the next button. */
    var submittedAt = 0L
    fun justSubmitted() = android.os.SystemClock.uptimeMillis() - submittedAt < 600
    /** What's typed in the address bar while editing. */
    var addressText by mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(""))
    var findOpen by mutableStateOf(false)
    var findQuery by mutableStateOf("")
    var findMatches by mutableIntStateOf(0)
    var findIndex by mutableIntStateOf(0)
    var fullscreenView by mutableStateOf<View?>(null)
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null
    var siteRequest by mutableStateOf<SiteRequest?>(null)

    private val desktopUserAgent: String by lazy {
        val chrome = Regex("""Chrome/([\d.]+)""").find(WebSettings.getDefaultUserAgent(activity))?.groupValues?.get(1) ?: "130.0.0.0"
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/$chrome Safari/537.36"
    }

    /** WebView's own user agent, without the "wv" marker some sites refuse. */
    private val mobileUserAgent: String by lazy {
        WebSettings.getDefaultUserAgent(activity).replace("; wv)", ")").replace(Regex("""Version/[\d.]+ """), "")
    }

    fun restore() {
        val saved = if (settings.restoreTabs) store.loadSession() else null
        if (saved != null && saved.first.isNotEmpty()) {
            saved.first.forEachIndexed { i, url -> tabs.add(Tab(url, saved.second.getOrElse(i) { "" })) }
            activeIndex = saved.third.coerceIn(0, tabs.lastIndex)
        } else {
            tabs.add(newTabPage())
        }
    }

    fun saveSession() = store.saveSession(tabs.map { it.url }, tabs.map { it.title }, activeIndex)

    private fun newTabPage(): Tab =
        if (settings.newTabPage == NewTabPage.Home && settings.homePage.isNotBlank()) Tab(Location.toUrl(settings.homePage, settings.searchEngine)) else Tab()

    /** Starts editing the address bar, with the page's address selected. */
    fun startEditing() {
        val full = if (active.isStart) "" else Location.display(active.url, settings.searchEngine)
        addressText = androidx.compose.ui.text.input.TextFieldValue(full, androidx.compose.ui.text.TextRange(0, full.length))
        switcherOpen = false
        editing = true
    }

    // --- Tabs -------------------------------------------------------------

    fun newTab(url: String? = null, background: Boolean = false, after: Int = tabs.size - 1): Tab {
        val tab = if (url == null) newTabPage() else Tab(url)
        val at = (after + 1).coerceIn(0, tabs.size)
        tabs.add(at, tab)
        if (!background) select(at) else if (at <= activeIndex) activeIndex++
        saveSession()
        return tab
    }

    fun select(index: Int) {
        if (index !in tabs.indices) return
        editing = false
        if (index != activeIndex) {
            captureThumbnail(active)
            active.webView?.onPause()
        }
        activeIndex = index
        active.webView?.onResume()
        stopFind()
        saveSession()
    }

    fun close(tab: Tab) {
        val i = tabs.indexOf(tab)
        if (i < 0) return
        destroyView(tab)
        tabs.removeAt(i)
        AdBlock.blockedCount.remove(tab.id)
        if (tabs.isEmpty()) tabs.add(newTabPage())
        if (activeIndex > i || activeIndex > tabs.lastIndex) activeIndex = (activeIndex - 1).coerceAtLeast(0)
        active.webView?.onResume()
        saveSession()
    }

    fun closeAll() {
        tabs.toList().forEach { destroyView(it) }
        tabs.clear()
        tabs.add(newTabPage())
        activeIndex = 0
        saveSession()
    }

    private fun destroyView(tab: Tab) {
        tab.webView?.let { wv ->
            (wv.parent as? ViewGroup)?.removeView(wv)
            wv.stopLoading()
            wv.destroy()
        }
        tab.webView = null
        tab.viewVersion++
    }

    /** Frees the memory of every tab but the one showing; they reload when chosen. */
    fun sleepBackgroundTabs() {
        tabs.forEachIndexed { i, t -> if (i != activeIndex && t.webView != null) destroyView(t) }
    }

    fun captureThumbnail(tab: Tab) {
        val wv = tab.webView ?: return
        if (wv.width <= 0 || wv.height <= 0) return
        runCatching {
            val scale = 0.35f
            val bmp = Bitmap.createBitmap((wv.width * scale).toInt().coerceAtLeast(1), (wv.height * scale).toInt().coerceAtLeast(1), Bitmap.Config.RGB_565)
            val c = Canvas(bmp)
            c.scale(scale, scale)
            c.translate(-wv.scrollX.toFloat(), -wv.scrollY.toFloat())
            wv.draw(c)
            tab.thumbnail = bmp
        }
    }

    // --- Navigation -------------------------------------------------------

    /** Loads what was typed into the address bar: an address, or a search. */
    fun go(text: String) {
        val t = text.trim()
        if (t.isEmpty()) return
        submittedAt = android.os.SystemClock.uptimeMillis()
        load(Location.toUrl(t, settings.searchEngine))
    }

    fun load(url: String, tab: Tab = active) {
        tab.url = url
        tab.progress = 5
        webViewFor(tab).loadUrl(url)
    }

    fun back(): Boolean {
        val wv = active.webView
        if (wv != null && wv.canGoBack()) { wv.goBack(); return true }
        if (!active.isStart && active.webView != null && settings.newTabPage == NewTabPage.Start) {
            // From the first page of a tab, Back returns to its start page.
            destroyView(active)
            active.url = ""
            active.title = ""
            active.progress = 100
            saveSession()
            return true
        }
        return false
    }

    fun forward() { active.webView?.takeIf { it.canGoForward() }?.goForward() }

    fun reload() {
        val t = active
        if (t.isStart) return
        if (t.loading) t.webView?.stopLoading() else (t.webView?.reload() ?: load(t.url, t))
    }

    fun goHome() {
        if (settings.homePage.isNotBlank()) go(settings.homePage) else {
            destroyView(active)
            active.url = ""
            active.title = ""
            active.progress = 100
        }
    }

    // --- Find in page -----------------------------------------------------

    fun find(query: String) {
        findQuery = query
        val wv = active.webView ?: return
        if (query.isEmpty()) { wv.clearMatches(); findMatches = 0; findIndex = 0 } else wv.findAllAsync(query)
    }

    fun findNext(forward: Boolean) { active.webView?.findNext(forward) }

    fun stopFind() {
        if (!findOpen) return
        findOpen = false
        findQuery = ""
        findMatches = 0
        active.webView?.clearMatches()
    }

    // --- Settings that reach into the pages ------------------------------

    fun applySettings() {
        tabs.forEach { t -> t.webView?.let { configure(it) } }
    }

    fun setDesktopSite(on: Boolean) {
        settings.update(desktopSite = on)
        applySettings()
        active.webView?.reload()
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun configure(wv: WebView) {
        wv.settings.apply {
            javaScriptEnabled = settings.javascript
            domStorageEnabled = true
            @Suppress("DEPRECATION")
            databaseEnabled = true
            loadWithOverviewMode = true
            useWideViewPort = true
            builtInZoomControls = true
            displayZoomControls = false
            setSupportZoom(true)
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = false
            mediaPlaybackRequiresUserGesture = true
            mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
            textZoom = settings.textZoom
            userAgentString = if (settings.desktopSite) desktopUserAgent else mobileUserAgent
            allowFileAccess = false
            allowContentAccess = true
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING)) {
            // Pages that have a dark look use it on their own when Ferrum is dark;
            // this darkens the rest too, only if asked to.
            WebSettingsCompat.setAlgorithmicDarkeningAllowed(wv.settings, settings.darkWebsites)
        }
    }

    // --- WebViews ---------------------------------------------------------

    /** The tab's WebView, made (and loaded with the tab's address) if it has none. */
    fun webViewFor(tab: Tab): WebView {
        tab.webView?.let { return it }
        val wv = createWebView(tab)
        tab.webView = wv
        tab.viewVersion++
        if (!tab.isStart) wv.loadUrl(tab.url)
        return wv
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView(tab: Tab): WebView {
        val wv = WebView(activity)
        wv.layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        configure(wv)
        CookieManager.getInstance().setAcceptThirdPartyCookies(wv, true)
        wv.webViewClient = Client(tab)
        wv.webChromeClient = Chrome(tab)
        wv.setDownloadListener { url, userAgent, disposition, mime, _ -> download(url, userAgent, disposition, mime) }
        wv.setFindListener { activeMatch, matches, _ ->
            if (tab === active) { findMatches = matches; findIndex = if (matches == 0) 0 else activeMatch + 1 }
        }
        return wv
    }

    private fun refreshNav(tab: Tab, view: WebView) {
        tab.canGoBack = view.canGoBack()
        tab.canGoForward = view.canGoForward()
    }

    private inner class Client(val tab: Tab) : WebViewClient() {
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            if (!settings.blockAds || request.isForMainFrame) return null
            val page = tab.pageHost
            if (page != null && page in settings.adsAllowed) return null
            if (!AdBlock.shouldBlock(request.url, page)) return null
            AdBlock.blockedCount.merge(tab.id, 1) { a, b -> a + b }
            activity.runOnUiThread { tab.blocked = AdBlock.blockedCount[tab.id] ?: 0 }
            return WebResourceResponse("text/plain", "utf-8", ByteArrayInputStream(ByteArray(0)))
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
            openOutside(view, request.url.toString())

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            tab.url = url
            tab.pageHost = Uri.parse(url).host?.lowercase()
            tab.progress = tab.progress.coerceAtMost(10)
            tab.favicon = favicon
            AdBlock.blockedCount[tab.id] = 0
            tab.blocked = 0
            refreshNav(tab, view)
        }

        override fun onPageFinished(view: WebView, url: String) {
            tab.progress = 100
            view.title?.takeIf { it.isNotBlank() && !it.startsWith("http") }?.let { tab.title = it }
            refreshNav(tab, view)
            store.recordVisit(url, tab.title)
            saveSession()
        }

        override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
            tab.url = url
            tab.pageHost = Uri.parse(url).host?.lowercase()
            refreshNav(tab, view)
        }

        override fun onRenderProcessGone(view: WebView, detail: android.webkit.RenderProcessGoneDetail): Boolean {
            // The page's process crashed or was reclaimed: drop the view; it reloads when shown.
            if (tab.webView === view) {
                (view.parent as? ViewGroup)?.removeView(view)
                view.destroy()
                tab.webView = null
                tab.viewVersion++
            }
            return true
        }
    }

    /** Hands links Ferrum can't show (mail, phone, app links) to other apps. */
    private fun openOutside(view: WebView, url: String): Boolean {
        val scheme = Uri.parse(url).scheme?.lowercase() ?: return false
        if (scheme in setOf("http", "https", "about", "data", "blob", "javascript", "file")) return false
        try {
            if (scheme == "intent") {
                val intent = Intent.parseUri(url, Intent.URI_INTENT_SCHEME).apply {
                    addCategory(Intent.CATEGORY_BROWSABLE)
                    component = null
                    selector = null
                }
                try {
                    activity.startActivity(intent)
                } catch (_: ActivityNotFoundException) {
                    intent.getStringExtra("browser_fallback_url")?.let { view.loadUrl(it) }
                        ?: intent.`package`?.let { pkg -> openApp(Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$pkg"))) }
                }
            } else {
                openApp(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE))
            }
        } catch (_: Exception) {
            toast("No app can open this link")
        }
        return true
    }

    private fun openApp(intent: Intent) {
        try {
            activity.startActivity(intent)
        } catch (_: ActivityNotFoundException) {
            toast("No app can open this link")
        }
    }

    private inner class Chrome(val tab: Tab) : WebChromeClient() {
        override fun onProgressChanged(view: WebView, newProgress: Int) {
            tab.progress = newProgress.coerceAtLeast(5)
            refreshNav(tab, view)
        }

        override fun onReceivedTitle(view: WebView, title: String?) {
            if (!title.isNullOrBlank() && !title.startsWith("http")) {
                tab.title = title
                store.updateTitle(tab.url, title)
            }
        }

        override fun onReceivedIcon(view: WebView, icon: Bitmap?) { tab.favicon = icon }

        override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
            if (!isUserGesture) return false // pop-ups nobody asked for
            val child = Tab()
            val wv = createWebView(child)
            child.webView = wv
            child.url = "about:blank"
            val at = tabs.indexOf(tab).let { if (it < 0) tabs.size else it + 1 }
            tabs.add(at, child)
            select(at)
            (resultMsg.obj as WebView.WebViewTransport).webView = wv
            resultMsg.sendToTarget()
            return true
        }

        override fun onCloseWindow(window: WebView) {
            tabs.firstOrNull { it.webView === window }?.let { close(it) }
        }

        override fun onShowFileChooser(view: WebView, callback: ValueCallback<Array<Uri>>, params: FileChooserParams): Boolean {
            activity.chooseFiles(params.createIntent(), params.mode == FileChooserParams.MODE_OPEN_MULTIPLE) { uris -> callback.onReceiveValue(uris) }
            return true
        }

        override fun onShowCustomView(view: View, callback: CustomViewCallback) {
            fullscreenCallback?.onCustomViewHidden()
            fullscreenView = view
            fullscreenCallback = callback
            activity.setFullscreen(true)
        }

        override fun onHideCustomView() = exitFullscreen()

        override fun onPermissionRequest(request: PermissionRequest) {
            activity.runOnUiThread {
                val wanted = request.resources
                val needs = mutableListOf<String>()
                val names = mutableListOf<String>()
                if (PermissionRequest.RESOURCE_VIDEO_CAPTURE in wanted) { needs += Manifest.permission.CAMERA; names += "camera" }
                if (PermissionRequest.RESOURCE_AUDIO_CAPTURE in wanted) { needs += Manifest.permission.RECORD_AUDIO; names += "microphone" }
                val grantable = wanted.filter {
                    it == PermissionRequest.RESOURCE_VIDEO_CAPTURE || it == PermissionRequest.RESOURCE_AUDIO_CAPTURE || it == PermissionRequest.RESOURCE_PROTECTED_MEDIA_ID
                }.toTypedArray()
                if (names.isEmpty()) {
                    // Protected media (DRM video) only: allow, as other browsers do.
                    if (grantable.isNotEmpty()) request.grant(grantable) else request.deny()
                    return@runOnUiThread
                }
                siteRequest = SiteRequest(request.origin.host ?: request.origin.toString(), names.joinToString(" and ")) { ok ->
                    siteRequest = null
                    if (!ok) request.deny() else activity.withPermissions(needs) { granted ->
                        if (granted) request.grant(grantable) else request.deny()
                    }
                }
            }
        }

        override fun onPermissionRequestCanceled(request: PermissionRequest) { siteRequest = null }

        override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) {
            siteRequest = SiteRequest(Uri.parse(origin).host ?: origin, "location") { ok ->
                siteRequest = null
                if (!ok) callback.invoke(origin, false, false) else activity.withPermissions(
                    listOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION),
                ) { granted -> callback.invoke(origin, granted, false) }
            }
        }

        override fun onGeolocationPermissionsHidePrompt() { siteRequest = null }

        /** No grey "play" picture over videos before they start. */
        override fun getDefaultVideoPoster(): Bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
    }

    fun exitFullscreen() {
        if (fullscreenView == null) return
        fullscreenView = null
        fullscreenCallback?.onCustomViewHidden()
        fullscreenCallback = null
        activity.setFullscreen(false)
    }

    // --- Downloads --------------------------------------------------------

    private fun download(url: String, userAgent: String?, disposition: String?, mime: String?) {
        if (!url.startsWith("http")) {
            toast("This file can't be downloaded")
            return
        }
        val name = URLUtil.guessFileName(url, disposition, mime)
        val start = {
            runCatching {
                val req = DownloadManager.Request(Uri.parse(url)).apply {
                    mime?.let { setMimeType(it) }
                    CookieManager.getInstance().getCookie(url)?.let { addRequestHeader("Cookie", it) }
                    addRequestHeader("User-Agent", userAgent ?: active.webView?.settings?.userAgentString ?: "")
                    setTitle(name)
                    setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
                }
                (activity.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(req)
                toast("Downloading $name")
            }.onFailure { toast("Couldn't download $name") }
            // A tab that only led to a download has nothing to show.
            if (active.isStart || active.title.isEmpty()) active.progress = 100
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            activity.withPermissions(listOf(Manifest.permission.WRITE_EXTERNAL_STORAGE)) { ok -> if (ok) start() else toast("Ferrum needs storage access to download") }
        } else start()
    }

    fun toast(text: String) = Toast.makeText(activity, text, Toast.LENGTH_SHORT).show()

    fun share(url: String, title: String) {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, url)
            putExtra(Intent.EXTRA_SUBJECT, title)
        }
        activity.startActivity(Intent.createChooser(send, null))
    }

    fun clearData(cache: Boolean, cookies: Boolean, history: Boolean) {
        if (cache) tabs.firstNotNullOfOrNull { it.webView }?.clearCache(true) ?: WebView(activity).apply { clearCache(true); destroy() }
        if (cookies) {
            CookieManager.getInstance().removeAllCookies(null)
            android.webkit.WebStorage.getInstance().deleteAllData()
        }
        if (history) {
            store.clearHistory()
            tabs.forEach { it.webView?.clearHistory() }
        }
    }
}

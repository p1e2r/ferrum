// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.ComponentCallbacks2
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.webkit.CookieManager
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : ComponentActivity() {
    lateinit var settings: Settings
    lateinit var store: Store
    lateinit var browser: Browser
    lateinit var updater: Updater

    private var fileCallback: ((Array<Uri>?) -> Unit)? = null
    private val fileLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val data = r.data
        val uris: Array<Uri>? = when {
            r.resultCode != RESULT_OK || data == null -> null
            data.clipData != null -> data.clipData!!.let { c -> Array(c.itemCount) { c.getItemAt(it).uri } }
            data.data != null -> arrayOf(data.data!!)
            else -> null
        }
        fileCallback?.invoke(uris)
        fileCallback = null
    }

    private var permissionCallback: ((Boolean) -> Unit)? = null
    private val permissionLauncher = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { result ->
        permissionCallback?.invoke(result.values.any { it })
        permissionCallback = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Lets chrome://inspect see pages in debug builds.
        if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)
        settings = Settings(this)
        store = Store(this)
        browser = Browser(this, settings, store)
        updater = Updater(this)
        AdBlock.load(this)
        applyNightMode()
        browser.restore()
        handle(intent)

        val day = 24L * 60 * 60 * 1000
        if (settings.checkUpdates && System.currentTimeMillis() - settings.lastUpdateCheck > day) {
            settings.lastUpdateCheck = System.currentTimeMillis()
            updater.check(quiet = true)
        }

        setContent {
            FerrumTheme(settings) {
                val dark = isDark(settings)
                LaunchedEffect(dark) {
                    WindowCompat.getInsetsController(window, window.decorView).apply {
                        isAppearanceLightStatusBars = !dark
                        isAppearanceLightNavigationBars = !dark
                    }
                }
                FerrumApp(this)
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /** Links opened from other apps open in a new tab. */
    private fun handle(intent: Intent?) {
        intent ?: return
        val url = when (intent.action) {
            Intent.ACTION_VIEW -> intent.dataString
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.trim()
            Intent.ACTION_WEB_SEARCH -> intent.getStringExtra("query")
            else -> null
        } ?: return
        intent.action = null // don't open it again when the activity is recreated
        browser.screen = Screen.Browser
        browser.switcherOpen = false
        browser.editing = false
        val target = if (url.startsWith("http://") || url.startsWith("https://")) url else Location.toUrl(url, settings.searchEngine)
        if (browser.active.isStart && browser.active.webView == null) browser.load(target) else browser.newTab(target, after = browser.activeIndex)
    }

    /** Light or dark as set in Ferrum, for pages as well (Android 12 and later). */
    fun applyNightMode() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val mode = when (settings.themeMode) {
                ThemeMode.System -> UiModeManager.MODE_NIGHT_AUTO
                ThemeMode.Light -> UiModeManager.MODE_NIGHT_NO
                ThemeMode.Dark -> UiModeManager.MODE_NIGHT_YES
            }
            runCatching { (getSystemService(UI_MODE_SERVICE) as UiModeManager).setApplicationNightMode(mode) }
        }
    }

    fun chooseFiles(intent: Intent, multiple: Boolean, callback: (Array<Uri>?) -> Unit) {
        fileCallback?.invoke(null)
        fileCallback = callback
        if (multiple) intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        try {
            fileLauncher.launch(intent)
        } catch (_: ActivityNotFoundException) {
            fileCallback = null
            callback(null)
        }
    }

    /** Runs `then(true)` once any of `permissions` is granted, asking Android first if needed. */
    fun withPermissions(permissions: List<String>, then: (Boolean) -> Unit) {
        if (permissions.any { ContextCompat.checkSelfPermission(this, it) == PackageManager.PERMISSION_GRANTED }) {
            then(true)
            return
        }
        permissionCallback?.invoke(false)
        permissionCallback = then
        permissionLauncher.launch(permissions.toTypedArray())
    }

    fun setFullscreen(on: Boolean) {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            if (on) {
                hide(WindowInsetsCompat.Type.systemBars())
                systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else {
                show(WindowInsetsCompat.Type.systemBars())
            }
        }
    }

    override fun onStart() {
        super.onStart()
        if (::browser.isInitialized) browser.tabs.firstNotNullOfOrNull { it.webView }?.resumeTimers()
    }

    override fun onResume() {
        super.onResume()
        browser.active.webView?.onResume()
    }

    override fun onPause() {
        browser.active.webView?.onPause()
        browser.saveSession()
        CookieManager.getInstance().flush()
        super.onPause()
    }

    override fun onStop() {
        // Timers are shared by every WebView: stopping them saves the battery while Ferrum is hidden.
        browser.tabs.firstNotNullOfOrNull { it.webView }?.pauseTimers()
        super.onStop()
    }

    @Suppress("DEPRECATION")
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) browser.sleepBackgroundTabs()
    }

    override fun onDestroy() {
        if (isFinishing) browser.tabs.forEach { t -> t.webView?.destroy(); t.webView = null }
        super.onDestroy()
    }
}

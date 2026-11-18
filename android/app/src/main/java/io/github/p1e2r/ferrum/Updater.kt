// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings as AndroidSettings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * One-tap updates from GitHub Releases, as on the desktop: finds the newest
 * release, downloads its Android APK, checks it against the release's
 * SHA256SUMS, then hands it to Android to install over this one.
 */
class Updater(private val context: Context) {
    sealed class State {
        object Idle : State()
        object Checking : State()
        object UpToDate : State()
        data class Available(val version: String, val apkUrl: String, val sumsUrl: String?) : State()
        data class Downloading(val version: String, val percent: Int) : State()
        data class Ready(val version: String, val file: File) : State()
        data class Failed(val message: String) : State()
    }

    var state by mutableStateOf<State>(State.Idle)
        private set

    private val main = Handler(Looper.getMainLooper())
    private fun post(s: State) = main.post { state = s }

    fun check(quiet: Boolean = false) {
        if (state is State.Checking || state is State.Downloading) return
        if (!quiet) state = State.Checking
        Thread {
            try {
                val json = JSONObject(get(LATEST).bufferedReader().use { it.readText() })
                val version = json.getString("tag_name").removePrefix("v")
                val assets = json.getJSONArray("assets")
                var apk: String? = null
                var sums: String? = null
                for (i in 0 until assets.length()) {
                    val a = assets.getJSONObject(i)
                    val name = a.getString("name")
                    if (name.endsWith("-android.apk")) apk = a.getString("browser_download_url")
                    if (name == "SHA256SUMS") sums = a.getString("browser_download_url")
                }
                post(
                    when {
                        !newer(version, BuildConfig.VERSION_NAME) -> if (quiet) State.Idle else State.UpToDate
                        apk == null -> if (quiet) State.Idle else State.Failed("Ferrum $version has no Android download yet")
                        else -> State.Available(version, apk, sums)
                    },
                )
            } catch (e: Exception) {
                post(if (quiet) State.Idle else State.Failed("Couldn't check for updates"))
            }
        }.start()
    }

    fun download() {
        val s = state as? State.Available ?: return
        state = State.Downloading(s.version, 0)
        Thread {
            try {
                val dir = File(context.cacheDir, "updates").apply { mkdirs(); listFiles()?.forEach { it.delete() } }
                val file = File(dir, "Ferrum-${s.version}-android.apk")
                val conn = open(s.apkUrl)
                val total = conn.contentLengthLong
                val digest = MessageDigest.getInstance("SHA-256")
                conn.inputStream.use { input ->
                    file.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        var done = 0L
                        var lastPercent = -1
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            out.write(buf, 0, n)
                            digest.update(buf, 0, n)
                            done += n
                            val percent = if (total > 0) (done * 100 / total).toInt() else 0
                            if (percent != lastPercent) { lastPercent = percent; post(State.Downloading(s.version, percent)) }
                        }
                    }
                }
                val hash = digest.digest().joinToString("") { "%02x".format(it) }
                if (s.sumsUrl != null) {
                    val sums = get(s.sumsUrl).bufferedReader().use { it.readText() }
                    val expected = sums.lineSequence().map { it.trim().split(Regex("\\s+")) }
                        .firstOrNull { it.size >= 2 && it.last().trimStart('*') == file.name }?.first()
                    if (expected != null && !expected.equals(hash, ignoreCase = true)) {
                        file.delete()
                        post(State.Failed("The download didn't match its checksum, so it wasn't installed"))
                        return@Thread
                    }
                }
                post(State.Ready(s.version, file))
                main.post { install() }
            } catch (e: Exception) {
                post(State.Failed("The download failed"))
            }
        }.start()
    }

    /** Opens Android's installer; first asks for "install unknown apps" if Ferrum hasn't got it. */
    fun install() {
        val s = state as? State.Ready ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !context.packageManager.canRequestPackageInstalls()) {
            context.startActivity(
                Intent(AndroidSettings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            return
        }
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", s.file)
        context.startActivity(
            Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    private fun open(url: String): HttpURLConnection {
        var u = url
        repeat(5) {
            val c = URL(u).openConnection() as HttpURLConnection
            c.connectTimeout = 15000
            c.readTimeout = 30000
            c.instanceFollowRedirects = true
            c.setRequestProperty("User-Agent", "Ferrum-Android/${BuildConfig.VERSION_NAME}")
            c.setRequestProperty("Accept", "application/vnd.github+json, */*")
            val code = c.responseCode
            if (code in 300..399) {
                u = c.getHeaderField("Location") ?: throw IllegalStateException("redirect")
                c.disconnect()
            } else if (code in 200..299) return c else throw IllegalStateException("HTTP $code")
        }
        throw IllegalStateException("too many redirects")
    }

    private fun get(url: String) = open(url).inputStream

    companion object {
        const val REPO = "p1e2r/ferrum"
        private const val LATEST = "https://api.github.com/repos/$REPO/releases/latest"

        /** Whether version `a` is newer than `b` ("1.10.0" > "1.9.3"). */
        fun newer(a: String, b: String): Boolean {
            fun parts(v: String) = v.substringBefore('-').split('.').map { it.toIntOrNull() ?: 0 } + listOf(0, 0, 0)
            val x = parts(a)
            val y = parts(b)
            for (i in 0 until 3) if (x[i] != y[i]) return x[i] > y[i]
            return false
        }
    }
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import android.content.Context
import android.net.Uri
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.thread

/**
 * Ad and tracker blocking, like the desktop app: a built-in list of ad and
 * tracking servers plus Peter Lowe's list (pgl.yoyo.org), cached in the app's
 * files and refreshed weekly. Requests to a listed server (or any of its
 * subdomains) from another site are answered with nothing.
 */
object AdBlock {
    private const val LIST_URL = "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=nohtml&showintro=0&mimetype=plaintext"
    private const val WEEK_MS = 7L * 24 * 60 * 60 * 1000

    /** The same built-in list as the desktop app (src/blocker.rs). */
    private val BUILT_IN = listOf(
        "doubleclick.net", "googlesyndication.com", "googleadservices.com", "adservice.google.com",
        "pagead2.googlesyndication.com", "adnxs.com", "adsrvr.org", "advertising.com", "criteo.com",
        "criteo.net", "pubmatic.com", "rubiconproject.com", "openx.net", "casalemedia.com", "taboola.com",
        "outbrain.com", "amazon-adsystem.com", "media.net", "moatads.com", "smartadserver.com", "adform.net",
        "yieldmo.com", "sharethrough.com", "teads.tv", "bidswitch.net", "3lift.com", "triplelift.com",
        "contextweb.com", "indexww.com", "zemanta.com", "revcontent.com", "mgid.com", "adroll.com",
        "serving-sys.com", "adsafeprotected.com", "doubleverify.com", "innovid.com", "spotxchange.com",
        "springserve.com", "lijit.com", "sovrn.com", "33across.com", "gumgum.com", "undertone.com",
        "yieldlab.net", "adition.com", "adtech.de", "popads.net", "propellerads.com", "exoclick.com",
        "adsterra.com", "trafficjunky.net", "juicyads.com", "zedo.com", "infolinks.com", "buysellads.com",
        "carbonads.net", "admixer.net", "adkernel.com", "smaato.net", "inmobi.com", "applovin.com",
        "unityads.unity3d.com", "chartboost.com", "vungle.com", "ironsrc.com", "mopub.com",
        "google-analytics.com", "googletagmanager.com", "googletagservices.com", "analytics.google.com",
        "scorecardresearch.com", "quantserve.com", "quantcount.com", "chartbeat.com", "chartbeat.net",
        "hotjar.com", "hotjar.io", "mouseflow.com", "fullstory.com", "crazyegg.com", "luckyorange.com",
        "clarity.ms", "mixpanel.com", "segment.io", "segment.com", "amplitude.com", "heap.io",
        "heapanalytics.com", "kissmetrics.com", "newrelic.com", "nr-data.net", "optimizely.com",
        "omtrdc.net", "demdex.net", "everesttech.net", "2o7.net", "bluekai.com", "krxd.net", "exelator.com",
        "eyeota.net", "rlcdn.com", "liadm.com", "tapad.com", "agkn.com", "addthis.com", "sharethis.com",
        "bounceexchange.com", "bouncex.net", "parsely.com", "parse.ly", "comscore.com", "nielsen.com",
        "imrworldwide.com", "statcounter.com", "yandex.ru/metrika", "mc.yandex.ru", "top-fwz1.mail.ru",
        "hm.baidu.com", "cnzz.com", "branch.io", "app-measurement.com", "appsflyer.com", "adjust.com",
        "kochava.com", "braze.com", "onesignal.com", "pushcrew.com", "pusher-ads.com", "trackcmp.net",
        "facebook.net", "connect.facebook.net", "pixel.facebook.com", "ads-twitter.com",
        "analytics.twitter.com", "static.ads-twitter.com", "ads.linkedin.com", "px.ads.linkedin.com",
        "snap.licdn.com", "analytics.tiktok.com", "ads.tiktok.com", "ct.pinterest.com", "bat.bing.com",
        "ads.yahoo.com", "analytics.yahoo.com", "sc-static.net", "tr.snapchat.com", "quora.com/_/ad",
        "redditstatic.com/ads", "alb.reddit.com", "pixel.reddit.com", "matomo.cloud", "piwik.pro",
        "kameleoon.com", "vwo.com", "visualwebsiteoptimizer.com", "zoominfo.com", "ws.zoominfo.com",
        "clearbit.com", "6sc.co", "demandbase.com", "bizible.com", "marketo.net", "mktoresp.com",
        "pardot.com", "hs-analytics.net", "hs-banner.com", "hsadspixel.net", "hubspot.net/analytics",
        "intercomcdn.com/analytics",
    )

    @Volatile private var domains: Set<String> = BUILT_IN.toHashSet()

    /** Blocked requests per page, for the counter. */
    val blockedCount = ConcurrentHashMap<Int, Int>()

    fun load(context: Context) {
        val cache = File(context.filesDir, "blocklist.txt")
        if (cache.exists()) merge(cache.readText())
        if (!cache.exists() || System.currentTimeMillis() - cache.lastModified() > WEEK_MS) {
            thread(name = "blocklist") {
                try {
                    val conn = URL(LIST_URL).openConnection() as HttpURLConnection
                    conn.connectTimeout = 15000
                    conn.readTimeout = 30000
                    val text = conn.inputStream.bufferedReader().use { it.readText() }
                    if (text.lines().count { it.contains('.') } > 100) {
                        cache.writeText(text)
                        merge(text)
                    }
                } catch (_: Exception) {
                }
            }
        }
    }

    private fun merge(text: String) {
        val more = text.lineSequence().map { it.trim().lowercase() }.filter { it.isNotEmpty() && !it.startsWith('#') && it.contains('.') }
        domains = domains + more
    }

    fun domainCount(): Int = domains.size

    /** The registrable part of a host, roughly: news.example.co.uk → example.co.uk. */
    fun baseDomain(host: String): String {
        val parts = host.lowercase().trimEnd('.').split('.')
        if (parts.size <= 2) return parts.joinToString(".")
        val secondLevel = parts[parts.size - 2]
        val twoPartSuffix = secondLevel.length <= 3 && parts.last().length == 2
        val keep = if (twoPartSuffix) 3 else 2
        return parts.takeLast(keep).joinToString(".")
    }

    /** Whether a request for `url`, made by a page on `pageHost`, should be blocked. */
    fun shouldBlock(url: Uri, pageHost: String?): Boolean {
        val host = url.host?.lowercase() ?: return false
        // Only requests to other sites.
        if (pageHost != null && baseDomain(host) == baseDomain(pageHost)) return false
        var h = host
        while (true) {
            if (h in domains) return true
            val dot = h.indexOf('.')
            if (dot < 0 || h.indexOf('.', dot + 1) < 0) return false
            h = h.substring(dot + 1)
        }
    }
}

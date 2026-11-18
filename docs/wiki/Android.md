# Ferrum for Android

Ferrum for Android lives in `android/`. It's a native Kotlin app built with Jetpack Compose, not a port of the Rust code: Android has no wry or egui. Pages are shown by **Android System WebView** (Chromium), and everything around the page is drawn in Compose.

The pieces that decide behaviour are ported line for line from the desktop, so both apps answer the same way. They are the calculator (`calc.rs`), address-bar text to address or search (`location.rs`) and the ad-block list and rules (`blocker.rs`).

## Files

| File | What it does |
|---|---|
| `MainActivity.kt` | The one activity: start-up, links from other apps (VIEW, SEND, WEB_SEARCH), file pickers, Android permissions, fullscreen video, pausing pages when hidden, sleeping background tabs on low memory |
| `Browser.kt` | `Browser` and `Tab`. Holds the tab list and what's on screen. Makes and configures each tab's WebView. Its `WebViewClient` does ad blocking, links to other apps and history; its `WebChromeClient` handles progress, titles, pop-ups as new tabs, file uploads, camera, microphone and location prompts, and fullscreen. Also downloads (DownloadManager), find in page, clearing data |
| `BrowserScreen.kt` | `FerrumApp`, the root composable and the Back button order. Also the top bar (address pill, ad-block shield, tab count, ⋮ menu), suggestions, find bar and the WebView host |
| `StartPage.kt` | New-tab page: clock, title, search box, bookmark and most-visited tiles (long-press to remove) |
| `TabSwitcher.kt` | The tab cards, with thumbnails taken when you leave a tab |
| `SettingsScreen.kt`, `ListScreen.kt` | Settings; bookmarks and history lists |
| `Store.kt` | `Settings` (SharedPreferences) and `Store` (bookmarks, history, open tabs as JSON in the app's files) |
| `Theme.kt` | Material 3 colours from Ferrum's colour themes, light and dark |
| `Updater.kt` | GitHub Releases update check, download, SHA-256 check against `SHA256SUMS`, and install |
| `AdBlock.kt`, `Calc.kt`, `Location.kt` | The ports described above |

All state is Compose state (`mutableStateOf`, `mutableStateListOf`), so changing a tab or a setting redraws whatever shows it. A tab's WebView is made only when the tab is first shown. Restored and background tabs cost nothing until you open them, and `sleepBackgroundTabs()` drops their views again when Android is short of memory.

## Dark mode

The theme setting (System, Light or Dark) sets Compose's colours. On Android 12 and later it also sets the app's night mode, which WebView follows: pages with a dark look of their own use it when Ferrum is dark. "Dark websites" additionally turns on WebView's algorithmic darkening for pages without one. It's off by default, as on the desktop.

## Building

```sh
cd android
./gradlew testDebugUnitTest   # calculator, addresses, ad-block, version tests
./gradlew assembleDebug       # app/build/outputs/apk/debug/app-debug.apk (package …ferrum.debug)
```

You need JDK 17 and the Android SDK, or just open `android/` in Android Studio. The version comes from `FERRUM_VERSION`, or from the desktop app's `Cargo.toml`, so both apps always share a version. The version code is `major*10000 + minor*100 + patch`.

## CI and releases

The `android` job in `.github/workflows/build.yml` runs the unit tests, builds the debug APK and, when the `ANDROID_KEYSTORE_PASSWORD` secret is set, builds the signed release APK as `Ferrum-<version>-android.apk`. On `main`, that APK goes into the release beside the desktop downloads and into `SHA256SUMS`.

The `android-test` job boots an Android 14 emulator and runs `android/ci/smoke.sh`. The script goes through the start page, calculator, a page load, the menu, a link from another app, the tab switcher, settings and dark mode, and saves a screenshot of each. It fails if the app crashed. Screenshots and logcat are in the run's `android-screenshots` artifact.

## Signing

Release APKs must always be signed with the same key, or phones won't install updates over the old version. The key is `android/signing/ferrum-release.jks.enc`: a PKCS#12 keystore, alias `ferrum`, encrypted with AES-256 (`openssl enc -aes-256-cbc -pbkdf2 -iter 200000`). The same password unlocks the file, the keystore and the key, and is stored only as the `ANDROID_KEYSTORE_PASSWORD` secret.

Certificate SHA-256 fingerprint, as registered in the Android Developer Console with package `io.github.p1e2r.ferrum`:

```
35:12:54:41:BE:AF:86:D1:4D:22:98:76:D6:03:00:7C:3A:8C:97:D3:69:91:17:83:60:9D:DA:7E:76:44:FD:E4
```

To sign locally, decrypt the keystore and put its path and password in `android/keystore.properties` (`storeFile`, `storePassword`, `keyAlias`, `keyPassword`). Both are git-ignored.

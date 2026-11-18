// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Ferrum contributors
package io.github.p1e2r.ferrum

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

@Composable
fun isDark(settings: Settings): Boolean = when (settings.themeMode) {
    ThemeMode.System -> isSystemInDarkTheme()
    ThemeMode.Light -> false
    ThemeMode.Dark -> true
}

/** Ferrum's colours: warm greys with the theme's accent, or Google's whites and greys. */
@Composable
fun FerrumTheme(settings: Settings, content: @Composable () -> Unit) {
    val dark = isDark(settings)
    val p = settings.palette
    val accent = Color(if (dark) p.darkAccent else p.accent)
    val google = p == Palette.Google
    val scheme = if (dark) {
        val bg = if (google) Color(0xFF202124) else Color(0xFF17171A)
        val surface = if (google) Color(0xFF292A2D) else Color(0xFF1F1F23)
        darkColorScheme(
            primary = accent, onPrimary = Color(0xFF1A1A1A),
            secondary = accent, onSecondary = Color(0xFF1A1A1A),
            background = bg, onBackground = Color(0xFFECEAE6),
            surface = surface, onSurface = Color(0xFFECEAE6),
            surfaceVariant = if (google) Color(0xFF303134) else Color(0xFF2A2A2F), onSurfaceVariant = Color(0xFFA9A6A0),
            surfaceContainer = surface, surfaceContainerHigh = if (google) Color(0xFF35363A) else Color(0xFF2B2B30),
            surfaceContainerLow = bg, surfaceContainerHighest = Color(0xFF34343A),
            outline = Color(0xFF55545A), outlineVariant = Color(0xFF38383E),
        )
    } else {
        val bg = if (google) Color.White else Color(0xFFF7F5F2)
        lightColorScheme(
            primary = accent, onPrimary = Color.White,
            secondary = accent, onSecondary = Color.White,
            background = bg, onBackground = Color(0xFF1F1D1A),
            surface = if (google) Color.White else Color(0xFFFCFBF9), onSurface = Color(0xFF1F1D1A),
            surfaceVariant = if (google) Color(0xFFF1F3F4) else Color(0xFFEDEAE5), onSurfaceVariant = Color(0xFF6B6760),
            surfaceContainer = if (google) Color(0xFFF8F9FA) else Color(0xFFF3F1ED), surfaceContainerHigh = Color.White,
            surfaceContainerLow = bg, surfaceContainerHighest = Color(0xFFE6E3DE),
            outline = Color(0xFFB8B3AC), outlineVariant = Color(0xFFE2DED8),
        )
    }
    val shapes = Shapes(
        extraSmall = RoundedCornerShape(14.dp),
        small = RoundedCornerShape(16.dp),
        medium = RoundedCornerShape(20.dp),
        large = RoundedCornerShape(26.dp),
        extraLarge = RoundedCornerShape(30.dp),
    )
    MaterialTheme(colorScheme = scheme, shapes = shapes, content = content)
}

/** A steady colour for a site's letter tile, from its name. */
fun tileColor(key: String): Color {
    val colors = listOf(0xFFD9622B, 0xFF1A73E8, 0xFF248F52, 0xFF9646B4, 0xFFD6466E, 0xFF0E8A8A, 0xFFB7791F, 0xFF5C6BC0)
    return Color(colors[(key.hashCode() and 0x7fffffff) % colors.size])
}

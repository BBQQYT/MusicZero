package com.musiczero.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val IndigoLight = Color(0xFF818CF8)
private val Indigo = Color(0xFF4F46E5)

/**
 * The app always renders dark; on Android 12+ Compose picks up the wallpaper
 * palette automatically through dynamicDarkColorScheme.
 */
@Composable
fun MusicZeroTheme(content: @Composable () -> Unit) {
    val scheme = darkColorScheme(
        primary = IndigoLight,
        secondary = Indigo,
        background = Color(0xFF0E0E12),
        surface = Color(0xFF16161C),
        surfaceContainer = Color(0xFF1C1C24),
    )
    MaterialTheme(colorScheme = scheme, content = content)
}

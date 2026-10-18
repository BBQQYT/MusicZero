package com.musiczero.app

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val DarkScheme = darkColorScheme(
    primary = Color(0xFFA5B4FC),
    onPrimary = Color(0xFF1E1B4B),
    secondary = Color(0xFFC7D2FE),
    onSecondary = Color(0xFF26234F),
    background = Color(0xFF0D0D12),
    onBackground = Color(0xFFE8E8EF),
    surface = Color(0xFF111117),
    onSurface = Color(0xFFE8E8EF),
    surfaceVariant = Color(0xFF26262F),
    onSurfaceVariant = Color(0xFFA7A7B4),
    surfaceContainer = Color(0xFF17171E),
    surfaceContainerHigh = Color(0xFF1D1D25),
    surfaceContainerHighest = Color(0xFF23232C),
    outline = Color(0xFF3A3A45),
)

private val LightScheme = lightColorScheme(
    primary = Color(0xFF4F46E5),
    onPrimary = Color(0xFFFFFFFF),
    secondary = Color(0xFF7C6FF0),
    background = Color(0xFFF7F7FB),
    onBackground = Color(0xFF17171D),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF17171D),
    surfaceVariant = Color(0xFFE4E4EC),
    onSurfaceVariant = Color(0xFF55555F),
    surfaceContainer = Color(0xFFEFF0F6),
    surfaceContainerHigh = Color(0xFFE8E8F1),
    surfaceContainerHighest = Color(0xFFE1E1EB),
    outline = Color(0xFFC3C3CD),
)

/** Rounded everything: the Retro look lives on big friendly corners. */
private val MzShapes = Shapes(
    extraSmall = RoundedCornerShape(6.dp),
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(22.dp),
    extraLarge = RoundedCornerShape(30.dp),
)

private val MzTypography = Typography(
    displaySmall = TextStyle(
        fontWeight = FontWeight.Bold,
        fontSize = 32.sp,
        lineHeight = 38.sp,
        letterSpacing = 0.sp,
    ),
    headlineSmall = TextStyle(
        fontWeight = FontWeight.Bold,
        fontSize = 24.sp,
        lineHeight = 30.sp,
    ),
    titleLarge = TextStyle(
        fontWeight = FontWeight.SemiBold,
        fontSize = 20.sp,
        lineHeight = 26.sp,
    ),
    titleMedium = TextStyle(
        fontWeight = FontWeight.SemiBold,
        fontSize = 16.sp,
        lineHeight = 22.sp,
    ),
    bodyLarge = TextStyle(
        fontWeight = FontWeight.Medium,
        fontSize = 16.sp,
        lineHeight = 22.sp,
    ),
    bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 20.sp),
    labelSmall = TextStyle(
        fontWeight = FontWeight.Medium,
        fontSize = 11.sp,
        lineHeight = 15.sp,
        letterSpacing = 0.4.sp,
    ),
    labelLarge = TextStyle(
        fontWeight = FontWeight.SemiBold,
        fontSize = 14.sp,
        lineHeight = 18.sp,
    ),
)

/**
 * Material You when the platform has it (Android 12+), an indigo fallback
 * otherwise. Both light and dark follow the system setting.
 */
@Composable
fun MusicZeroTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val scheme = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> DarkScheme
        else -> LightScheme
    }
    MaterialTheme(
        colorScheme = scheme,
        typography = MzTypography,
        shapes = MzShapes,
        content = content,
    )
}

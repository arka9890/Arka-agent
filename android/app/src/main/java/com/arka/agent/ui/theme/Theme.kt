package com.arka.agent.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Palet "Claymorphism": ungu lembut sebagai aksen, permukaan bertingkat
 * dengan sudut membulat besar, dan bayangan dua arah (gelap di bawah, terang
 * di atas) supaya kartu terlihat seperti tanah liat yang ditekan.
 */
data class ClayColors(
    val background: Color,
    val surface: Color,
    val surfaceRaised: Color,
    val accent: Color,
    val accentSoft: Color,
    val onSurface: Color,
    val muted: Color,
    val shadowDark: Color,
    val shadowLight: Color,
    val success: Color,
    val danger: Color,
    val userBubble: Color,
    val aiBubble: Color,
)

private val DarkClay = ClayColors(
    background = Color(0xFF0E1015),
    surface = Color(0xFF171A22),
    surfaceRaised = Color(0xFF1E222C),
    accent = Color(0xFF8B7CF6),
    accentSoft = Color(0xFF2A2547),
    onSurface = Color(0xFFEDEFF5),
    muted = Color(0xFF8E94A6),
    shadowDark = Color(0xFF05060A),
    shadowLight = Color(0xFF262B37),
    success = Color(0xFF4ADE80),
    danger = Color(0xFFF87171),
    userBubble = Color(0xFF8B7CF6),
    aiBubble = Color(0xFF1E222C),
)

private val LightClay = ClayColors(
    background = Color(0xFFF2F3F8),
    surface = Color(0xFFFBFBFE),
    surfaceRaised = Color(0xFFFFFFFF),
    accent = Color(0xFF6C5CE7),
    accentSoft = Color(0xFFE4E0FF),
    onSurface = Color(0xFF171A22),
    muted = Color(0xFF6B7280),
    shadowDark = Color(0xFFC7CAD8),
    shadowLight = Color(0xFFFFFFFF),
    success = Color(0xFF16A34A),
    danger = Color(0xFFDC2626),
    userBubble = Color(0xFF6C5CE7),
    aiBubble = Color(0xFFFFFFFF),
)

val LocalClayColors = staticCompositionLocalOf { DarkClay }
val LocalFontScale = staticCompositionLocalOf { 1f }

/** Ukuran font dari Settings: small / medium / large. */
fun fontScaleFor(name: String?): Float = when (name) {
    "small" -> 0.9f
    "large" -> 1.15f
    else -> 1f
}

private fun baseTypography(scale: Float): Typography {
    val base = Typography()
    // Bug lama: parameter fontScale dari Setelan tidak pernah dipakai di sini,
    // jadi memilih "Kecil"/"Besar" tidak mengubah apa pun di layar.
    fun TextStyle.scaled() = copy(fontSize = fontSize * scale, lineHeight = lineHeight * scale)
    return Typography(
        displayLarge = base.displayLarge.scaled(),
        displayMedium = base.displayMedium.scaled(),
        displaySmall = base.displaySmall.scaled(),
        headlineLarge = base.headlineLarge.scaled(),
        headlineMedium = base.headlineMedium.scaled(),
        headlineSmall = base.headlineSmall.scaled(),
        titleLarge = base.titleLarge.scaled().copy(fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.scaled().copy(fontWeight = FontWeight.SemiBold),
        titleSmall = base.titleSmall.scaled(),
        bodyLarge = base.bodyLarge.scaled(),
        bodyMedium = base.bodyMedium.scaled(),
        bodySmall = base.bodySmall.scaled(),
        labelLarge = base.labelLarge.scaled().copy(fontWeight = FontWeight.Medium),
        labelMedium = base.labelMedium.scaled(),
        labelSmall = base.labelSmall.scaled().copy(fontSize = 11.sp * scale, letterSpacing = 0.4.sp),
    )
}

@Composable
fun ArkaTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    fontScale: Float = 1f,
    content: @Composable () -> Unit,
) {
    val clay = if (darkTheme) DarkClay else LightClay
    val scheme = if (darkTheme) {
        darkColorScheme(
            primary = clay.accent,
            onPrimary = Color.White,
            background = clay.background,
            onBackground = clay.onSurface,
            surface = clay.surface,
            onSurface = clay.onSurface,
            surfaceVariant = clay.surfaceRaised,
            error = clay.danger,
        )
    } else {
        lightColorScheme(
            primary = clay.accent,
            onPrimary = Color.White,
            background = clay.background,
            onBackground = clay.onSurface,
            surface = clay.surface,
            onSurface = clay.onSurface,
            surfaceVariant = clay.surfaceRaised,
            error = clay.danger,
        )
    }

    CompositionLocalProvider(
        LocalClayColors provides clay,
        LocalFontScale provides fontScale,
    ) {
        MaterialTheme(
            colorScheme = scheme,
            typography = baseTypography(fontScale),
            content = content,
        )
    }
}

/** Shorthand yang dipakai di seluruh UI. */
val clay: ClayColors
    @Composable get() = LocalClayColors.current

/** Jarak standar supaya layout tidak "menempel" di tepi. */
object Space {
    val xs = 4.dp
    val sm = 8.dp
    val md = 12.dp
    val lg = 16.dp
    val xl = 24.dp
}

package com.arka.agent.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.arka.agent.ui.theme.clay

/**
 * Permukaan bergaya clay: sudut membulat besar, bayangan jatuh yang lembut,
 * dan garis tepi terang di sisi atas supaya terlihat timbul.
 *
 * Dipakai semua kartu di aplikasi (bubble chat, kartu provider, panel
 * terminal) — satu tempat untuk mengubah "rasa" visual aplikasi.
 */
@Composable
fun ClayCard(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(22.dp),
    color: Color = clay.surface,
    elevation: Dp = 10.dp,
    contentPadding: Dp = 16.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        modifier = modifier.shadow(
            elevation = elevation,
            shape = shape,
            ambientColor = clay.shadowDark,
            spotColor = clay.shadowDark,
        ),
        shape = shape,
        color = color,
        border = BorderStroke(1.dp, clay.shadowLight.copy(alpha = 0.08f)),
    ) {
        Column(modifier = Modifier.padding(contentPadding), content = content)
    }
}

/**
 * Kotak kecil berwarna lembut untuk label/status.
 *
 * Kalau [onClick] diisi, pill menjadi tombol (ripple + area klik penuh) —
 * dipakai untuk "Mulai sesi baru" dan melepas lampiran chat.
 *
 * Catatan: memakai `Modifier.clickable`, bukan overload `Surface(onClick=…)`
 * yang sudah deprecated di Material3.
 */
@Composable
fun ClayPill(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val shape = RoundedCornerShape(50)
    val background = color.copy(alpha = 0.16f)
    val border = BorderStroke(1.dp, color.copy(alpha = 0.35f))
    val clickModifier = if (onClick != null) {
        modifier.clip(shape).clickable(onClick = onClick)
    } else {
        modifier
    }

    Surface(
        modifier = clickModifier,
        shape = shape,
        color = background,
        border = border,
    ) {
        androidx.compose.material3.Text(
            text = text,
            style = androidx.compose.material3.MaterialTheme.typography.labelSmall,
            color = color,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
        )
    }
}

/** Latar bergradasi halus supaya warna dasar tidak terlihat rata. */
@Composable
fun clayBackground(): Modifier =
    Modifier.background(clay.background)

package com.arka.agent.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
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

/** Kotak kecil berwarna lembut untuk label/status. */
@Composable
fun ClayPill(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(50),
        color = color.copy(alpha = 0.16f),
        border = BorderStroke(1.dp, color.copy(alpha = 0.35f)),
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

package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.arka.agent.core.ApprovalRequest
import com.arka.agent.core.QuestionRequest
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay
import com.arka.agent.ui.tr

/** Kartu persetujuan: model menunggu sampai pengguna menekan Izinkan/Tolak. */
@Composable
fun ApprovalCardDialog(
    request: ApprovalRequest,
    onApprove: () -> Unit,
    onReject: () -> Unit,
) {
    Dialog(
        onDismissRequest = onReject,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x99000000)),
            contentAlignment = Alignment.Center,
        ) {
            ClayCard(
                Modifier
                    .fillMaxWidth()
                    .padding(Space.lg),
            ) {
                Text(
                    tr("Persetujuan aksi", "Action approval", "操作の承認"),
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(Space.sm))
                Text(
                    request.action,
                    style = MaterialTheme.typography.bodyMedium,
                    color = clay.accent,
                )
                if (request.target.isNotBlank()) {
                    Text(
                        "Target: ${request.target}",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
                if (request.detail.isNotBlank()) {
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        request.detail,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.verticalScroll(rememberScrollState()).heightIn(max = 220.dp),
                    )
                }
                Spacer(Modifier.height(Space.md))
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    TextButton(onClick = onReject) {
                        Text(tr("Tolak", "Reject", "拒否"))
                    }
                    Spacer(Modifier.width(Space.xs))
                    Button(onClick = onApprove) {
                        Text(tr("Izinkan", "Allow", "許可"))
                    }
                }
            }
        }
    }
}

/** Kartu pertanyaan pilihan ganda dari model. */
@Composable
fun QuestionCardDialog(
    request: QuestionRequest,
    onAnswer: (Int) -> Unit,
) {
    Dialog(
        onDismissRequest = { onAnswer(-1) },
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x99000000)),
            contentAlignment = Alignment.Center,
        ) {
            ClayCard(
                Modifier
                    .fillMaxWidth()
                    .padding(Space.lg),
            ) {
                Text(
                    tr("Pertanyaan dari AI", "Question from AI", "AIからの質問"),
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(Space.sm))
                Text(request.question, style = MaterialTheme.typography.bodyMedium)
                Spacer(Modifier.height(Space.md))
                Column(
                    Modifier
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(Space.xs),
                ) {
                    request.options.forEachIndexed { index, option ->
                        Text(
                            option,
                            style = MaterialTheme.typography.bodySmall,
                            color = clay.accent,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(clay.accentSoft)
                                .clickable { onAnswer(index) }
                                .padding(Space.sm),
                        )
                    }
                }
                Spacer(Modifier.height(Space.sm))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = { onAnswer(-1) }) {
                        Text(tr("Batal", "Cancel", "キャンセル"))
                    }
                }
            }
        }
    }
}

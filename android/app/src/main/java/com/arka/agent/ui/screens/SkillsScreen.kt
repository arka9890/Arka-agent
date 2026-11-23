package com.arka.agent.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import com.arka.agent.MainViewModel
import com.arka.agent.core.Skill
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay
import com.arka.agent.ui.tr

/**
 * Kelola skill agen berformat Claude (`SKILL.md`).
 *
 * Skill yang aktif disisipkan ke system prompt saat chat, sehingga model
 * mengetahui deskripsi dan instruksi lengkapnya tanpa tool tambahan.
 */
@Composable
fun SkillsScreen(vm: MainViewModel) {
    val skills by vm.skills.collectAsState()
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent(),
    ) { uri ->
        if (uri != null) vm.importSkillFromUri(uri)
    }

    LazyColumn(
        Modifier
            .fillMaxSize()
            .padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        item {
            ClayCard(Modifier.fillMaxWidth()) {
                Text(
                    tr("Skill agen", "Agent skills", "エージェントスキル"),
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(Space.xs))
                Text(
                    tr(
                        "Impor file SKILL.md berformat Claude (frontmatter name + description, lalu instruksi). Skill yang aktif otomatis disertakan saat chat.",
                        "Import Claude-style SKILL.md files (name + description frontmatter, then instructions). Enabled skills are included in every chat.",
                        "Claude形式のSKILL.md（name + description の frontmatter と本文）を読み込みます。有効なスキルはチャット時に自動で使われます。",
                    ),
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
                Spacer(Modifier.height(Space.sm))
                Button(onClick = { launcher.launch("*/*") }) {
                    Icon(Icons.Outlined.Add, null)
                    Spacer(Modifier.width(Space.xs))
                    Text(tr("Impor SKILL.md", "Import SKILL.md", "SKILL.mdを読み込む"))
                }
            }
        }

        if (skills.isEmpty()) {
            item {
                Text(
                    tr(
                        "Belum ada skill. Impor file .md pertamamu di atas.",
                        "No skills yet. Import your first .md file above.",
                        "スキルはまだありません。上から .md ファイルを読み込んでください。",
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = clay.muted,
                    modifier = Modifier.padding(Space.sm),
                )
            }
        }

        items(skills, key = { it.id }) { skill ->
            SkillCard(
                skill = skill,
                onEnabledChange = { vm.setSkillEnabled(skill.id, it) },
                onDelete = { vm.deleteSkill(skill.id) },
            )
        }
    }
}

@Composable
private fun SkillCard(
    skill: Skill,
    onEnabledChange: (Boolean) -> Unit,
    onDelete: () -> Unit,
) {
    ClayCard(Modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(skill.name, style = MaterialTheme.typography.bodyMedium)
                if (skill.description.isNotBlank()) {
                    Text(
                        skill.description,
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                        maxLines = 3,
                    )
                }
            }
            Switch(checked = skill.enabled, onCheckedChange = onEnabledChange)
        }
        if (skill.content.isNotBlank()) {
            Spacer(Modifier.height(Space.xs))
            Text(
                skill.content.take(300),
                style = MaterialTheme.typography.labelSmall.copy(
                    fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                ),
                color = clay.muted,
                maxLines = 4,
            )
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            TextButton(onClick = onDelete) {
                Icon(Icons.Outlined.Delete, null, tint = clay.danger)
                Text(tr("Hapus", "Delete", "削除"), color = clay.danger)
            }
        }
    }
}

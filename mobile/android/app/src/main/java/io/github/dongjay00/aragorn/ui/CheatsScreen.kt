package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel

/** 지금 게임의 치트 목록. 게임 지니·게임샤크 코드를 더하고 켜고 끈다. 목록은 세이브 옆 `.cht`에 저장된다. */
@Composable
fun CheatsScreen(viewModel: GameViewModel, onBack: () -> Unit) {
    val cheats by viewModel.cheats.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    var code by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }

    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onBack) { Text("← 돌아가기") }
                Text("치트", style = MaterialTheme.typography.titleMedium)
            }
            LazyColumn(Modifier.weight(1f).padding(horizontal = 16.dp)) {
                if (cheats.isEmpty()) {
                    item { Text("아직 치트가 없습니다.", Modifier.padding(vertical = 8.dp)) }
                }
                itemsIndexed(cheats) { index, cheat ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(cheat.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (cheat.label != cheat.code) Text(cheat.code, style = Mono)
                        }
                        Switch(
                            checked = cheat.enabled,
                            onCheckedChange = { viewModel.setCheatEnabled(index, it) },
                        )
                        TextButton(onClick = { viewModel.removeCheat(index) }) { Text("지우기") }
                    }
                    HorizontalDivider()
                }
                item {
                    Spacer(Modifier.height(16.dp))
                    OutlinedTextField(
                        value = code,
                        onValueChange = { code = it },
                        label = { Text("코드") },
                        placeholder = { Text("00A-17B-C49 또는 010138CD") },
                        textStyle = Mono,
                        keyboardOptions = KeyboardOptions(
                            capitalization = KeyboardCapitalization.Characters,
                            autoCorrectEnabled = false,
                        ),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedTextField(
                        value = name,
                        onValueChange = { name = it },
                        label = { Text("이름 (선택)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Spacer(Modifier.height(8.dp))
                    Button(
                        onClick = {
                            if (viewModel.addCheat(code, name)) {
                                code = ""
                                name = ""
                            }
                        },
                        enabled = code.isNotBlank(),
                    ) {
                        Text("더하기")
                    }
                    Text(
                        "게임 지니(ABC-DEF-GHI)와 게임샤크(01VVAAAA) 코드를 씁니다. " +
                            "여러 코드를 줄을 바꿔 쓰거나 + 로 이으면 한 치트로 켜고 끕니다.",
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
            }
            notice?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(16.dp),
                )
            }
        }
    }
}

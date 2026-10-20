package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel
import io.github.dongjay00.aragorn.core.LinkState
import kotlinx.coroutines.delay

/** 링크 케이블: 같은 와이파이의 다른 Aragorn과 통신 교환·대전을 한다. */
@Composable
fun LinkScreen(viewModel: GameViewModel, onBack: () -> Unit) {
    val emulator by viewModel.emulator.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    var state by remember { mutableStateOf<LinkState?>(null) }
    var address by rememberSaveable { mutableStateOf("") }
    // 연결 상태는 다른 스레드에서 바뀌므로 자주 다시 읽는다.
    LaunchedEffect(emulator) {
        while (true) {
            state = emulator?.linkState()
            delay(300)
        }
    }

    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onBack) { Text("← 돌아가기") }
                Text("링크 케이블", style = MaterialTheme.typography.titleMedium)
            }
            Column(Modifier.padding(horizontal = 16.dp)) {
                Text(
                    "같은 와이파이의 다른 Aragorn(폰이나 PC)과 통신 교환·대전을 합니다. " +
                        "한쪽이 \"기다리기\"를 누르고, 다른 쪽이 보이는 주소로 연결하세요.",
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(16.dp))
                when (val current = state) {
                    is LinkState.Waiting -> {
                        Text("상대를 기다리는 중", style = MaterialTheme.typography.titleSmall)
                        Text("상대 기기에서 이 주소로 연결하세요: ${current.address}", style = Mono)
                        Spacer(Modifier.height(8.dp))
                        OutlinedButton(onClick = viewModel::linkDisconnect) { Text("그만두기") }
                    }
                    is LinkState.Connecting -> {
                        Text("${current.address}에 연결하는 중…")
                        OutlinedButton(onClick = viewModel::linkDisconnect) { Text("그만두기") }
                    }
                    is LinkState.Connected -> {
                        Text("${current.peer}와 연결됨", style = MaterialTheme.typography.titleSmall)
                        Text(
                            "게임 안에서 통신 교환이나 대전을 시작하세요. 교환·대전 중에는 되감기를 쓸 수 없습니다.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Spacer(Modifier.height(8.dp))
                        OutlinedButton(onClick = viewModel::linkDisconnect) { Text("끊기") }
                    }
                    null -> {
                        Button(onClick = viewModel::linkHost, enabled = emulator != null) { Text("기다리기") }
                        Spacer(Modifier.height(16.dp))
                        OutlinedTextField(
                            value = address,
                            onValueChange = { address = it },
                            label = { Text("상대 주소") },
                            placeholder = { Text("192.168.0.5") },
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Spacer(Modifier.height(8.dp))
                        Button(
                            onClick = { viewModel.linkConnect(address) },
                            enabled = emulator != null && address.isNotBlank(),
                        ) {
                            Text("연결")
                        }
                    }
                }
            }
            Spacer(Modifier.weight(1f))
            notice?.let { Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp)) }
        }
    }
}

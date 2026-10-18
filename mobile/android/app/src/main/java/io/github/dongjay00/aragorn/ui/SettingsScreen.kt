package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel
import io.github.dongjay00.aragorn.UpdateState
import io.github.dongjay00.aragorn.core.FastSpeed
import io.github.dongjay00.aragorn.core.InputDevice
import io.github.dongjay00.aragorn.core.Speed
import io.github.dongjay00.aragorn.core.actionLabel
import io.github.dongjay00.aragorn.core.allActions
import io.github.dongjay00.aragorn.core.appVersion
import io.github.dongjay00.aragorn.core.fastSpeedLabel
import io.github.dongjay00.aragorn.core.speedLabel

/** 데스크톱 설정 창과 같은 항목: 키·게임패드 매핑, 기본 속도, 배속 속도, 음소거, 업데이트. */
@Composable
fun SettingsScreen(viewModel: GameViewModel, onBack: () -> Unit) {
    val capturing by viewModel.capturing.collectAsStateWithLifecycle()
    val bindingsVersion by viewModel.bindingsVersion.collectAsStateWithLifecycle()
    val baseSpeed by viewModel.baseSpeed.collectAsStateWithLifecycle()
    val fastSpeed by viewModel.fastSpeed.collectAsStateWithLifecycle()
    val muted by viewModel.muted.collectAsStateWithLifecycle()
    val update by viewModel.update.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()

    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onBack) { Text("← 돌아가기") }
                Text("설정", style = MaterialTheme.typography.titleMedium)
            }
            Column(
                Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp),
            ) {
                Section("키 설정")
                Text(
                    "칸을 누른 뒤 키보드나 게임패드 버튼을 누르세요. F1–F10은 스테이트 단축키라 지정할 수 없습니다.",
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(8.dp))
                Row {
                    Text("기능", Modifier.weight(1f), style = MaterialTheme.typography.labelLarge)
                    Text("키보드", Modifier.weight(1.2f), style = MaterialTheme.typography.labelLarge)
                    Text("게임패드", Modifier.weight(1.2f), style = MaterialTheme.typography.labelLarge)
                }
                // bindingsVersion이 바뀌면 다시 읽는다.
                key(bindingsVersion) {
                    for (action in allActions()) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(actionLabel(action), Modifier.weight(1f))
                            for (device in listOf(InputDevice.KEYBOARD, InputDevice.GAMEPAD)) {
                                BindingCell(
                                    name = viewModel.settings.binding(device, action),
                                    waiting = capturing == device to action,
                                    onClick = { viewModel.toggleCapture(device, action) },
                                    modifier = Modifier.weight(1.2f),
                                )
                            }
                        }
                    }
                }
                if (capturing != null) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("키를 누르세요", Modifier.weight(1f))
                        TextButton(onClick = viewModel::clearCaptured) { Text("지우기") }
                        TextButton(onClick = viewModel::cancelCapture) { Text("취소") }
                    }
                }

                Section("속도")
                Text("기본 속도")
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (speed in Speed.entries) {
                        FilterChip(
                            selected = speed == baseSpeed,
                            onClick = { viewModel.setBaseSpeed(speed) },
                            label = { Text(speedLabel(speed)) },
                        )
                    }
                }
                Text("배속 속도 (배속 버튼·키를 누르는 동안)")
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (speed in FastSpeed.entries) {
                        FilterChip(
                            selected = speed == fastSpeed,
                            onClick = { viewModel.setFastSpeed(speed) },
                            label = { Text(fastSpeedLabel(speed)) },
                        )
                    }
                }
                Text(
                    "2배·4배에서는 소리가 빠르고 높게 들립니다. 무제한에서는 소리가 나지 않습니다.",
                    style = MaterialTheme.typography.bodySmall,
                )

                Section("소리")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("음소거", Modifier.weight(1f))
                    Switch(checked = muted, onCheckedChange = viewModel::setMuted)
                }

                Section("세이브")
                Text(
                    "파일 앱의 \"Aragorn\" 항목에 데스크톱과 같은 형식(.sav, .ss0–.ss9)으로 저장합니다. " +
                        "데스크톱의 세이브를 ROM 파일 이름에 맞춰 넣으면 이어서 할 수 있습니다. " +
                        "실행 중인 게임의 세이브를 바꾸면 게임을 닫습니다.",
                    style = MaterialTheme.typography.bodySmall,
                )

                Section("업데이트")
                Text("현재 버전 ${appVersion()}")
                Text(updateText(update, viewModel.updateEnabled), style = MaterialTheme.typography.bodySmall)
                OutlinedButton(
                    onClick = { viewModel.checkForUpdate(manual = true) },
                    enabled = update !is UpdateState.Checking,
                ) {
                    Text("업데이트 확인")
                }

                Spacer(Modifier.height(16.dp))
                HorizontalDivider()
                TextButton(onClick = viewModel::resetInput) { Text("키 설정과 속도를 기본값으로") }
                Spacer(Modifier.height(24.dp))
            }
            notice?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(16.dp),
                )
            }
        }
    }
}

@Composable
private fun Section(title: String) {
    Spacer(Modifier.height(20.dp))
    Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
    Spacer(Modifier.height(4.dp))
}

@Composable
private fun BindingCell(name: String, waiting: Boolean, onClick: () -> Unit, modifier: Modifier) {
    val text = when {
        waiting -> "입력 대기…"
        name.isEmpty() -> "없음"
        else -> name
    }
    TextButton(
        onClick = onClick,
        modifier = modifier.then(
            if (waiting) Modifier.background(MaterialTheme.colorScheme.secondaryContainer) else Modifier,
        ),
    ) {
        Text(text, style = Mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
    Spacer(Modifier.width(4.dp))
}

private fun updateText(state: UpdateState, enabled: Boolean): String = when {
    !enabled -> "이 빌드는 업데이트를 확인하지 않습니다"
    else -> when (state) {
        UpdateState.Idle -> ""
        UpdateState.Checking -> "확인하는 중…"
        UpdateState.UpToDate -> "최신 버전입니다"
        is UpdateState.Failed -> "확인하지 못했습니다: ${state.reason}"
        is UpdateState.Available -> "새 버전 ${state.version}이(가) 있습니다"
        is UpdateState.Required -> "새 버전 ${state.version}으로 업데이트해야 합니다"
    }
}

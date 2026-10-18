package io.github.dongjay00.aragorn

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.core.InputDevice
import io.github.dongjay00.aragorn.ui.DebuggerScreen
import io.github.dongjay00.aragorn.ui.GameScreen
import io.github.dongjay00.aragorn.ui.SettingsScreen

class MainActivity : ComponentActivity() {
    private val viewModel: GameViewModel by viewModels()
    /** 게임패드마다 축 상태를 따로 둔다. */
    private val axes = mutableMapOf<Int, AxisButtons>()

    private val openRom = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(viewModel::openRom)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        AndroidContext.init(applicationContext)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                AragornApp(
                    viewModel,
                    // .gb/.gbc에는 등록된 MIME 타입이 없어 모든 파일을 보여 준다.
                    onOpenRom = { openRom.launch(arrayOf("*/*")) },
                    onOpenUrl = ::openUrl,
                )
            }
        }
    }

    override fun onStart() {
        super.onStart()
        viewModel.setVisible(true)
    }

    override fun onStop() {
        viewModel.setVisible(false)
        super.onStop()
    }

    /**
     * 블루투스·USB 게임패드와 키보드. Compose가 방향키를 포커스 이동에 쓰기 전에 먼저 받는다.
     * 게임 화면이나 키 설정 중이 아니면 그대로 넘겨 글자 입력 등이 되게 한다.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val code = event.keyCode
        if (GameInput.isSystemKey(code)) return super.dispatchKeyEvent(event)
        val pressed = when (event.action) {
            KeyEvent.ACTION_DOWN -> true
            KeyEvent.ACTION_UP -> false
            else -> return super.dispatchKeyEvent(event)
        }
        val device = GameInput.deviceOf(event) ?: return super.dispatchKeyEvent(event)
        val slot = GameInput.SLOT_KEYS.indexOf(code)
        if (slot >= 0 && device == InputDevice.KEYBOARD) {
            if (pressed && event.repeatCount == 0) viewModel.onSlotKey(slot, load = event.isShiftPressed)
            return true
        }
        val handled = viewModel.onKey(device, GameInput.keyName(code), pressed, event.repeatCount > 0)
        return handled || super.dispatchKeyEvent(event)
    }

    /** 게임패드의 방향 패드(hat)와 트리거 축. */
    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        val device = GameInput.deviceOf(event)
        if (device != InputDevice.GAMEPAD ||
            event.action != MotionEvent.ACTION_MOVE
        ) {
            return super.dispatchGenericMotionEvent(event)
        }
        var handled = false
        axes.getOrPut(event.deviceId) { AxisButtons() }.update(event) { name, pressed ->
            handled = viewModel.onKey(device, name, pressed) || handled
        }
        return handled || super.dispatchGenericMotionEvent(event)
    }

    private fun openUrl(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (_: ActivityNotFoundException) {
            // 브라우저가 없는 기기
        }
    }
}

@Composable
private fun AragornApp(viewModel: GameViewModel, onOpenRom: () -> Unit, onOpenUrl: (String) -> Unit) {
    val page by viewModel.page.collectAsStateWithLifecycle()
    val update by viewModel.update.collectAsStateWithLifecycle()
    BackHandler(enabled = page != Page.GAME) { viewModel.showPage(Page.GAME) }
    when (page) {
        Page.GAME -> GameScreen(viewModel, onOpenRom)
        Page.SETTINGS -> SettingsScreen(viewModel, onBack = { viewModel.showPage(Page.GAME) })
        Page.DEBUGGER -> DebuggerScreen(viewModel, onBack = { viewModel.showPage(Page.GAME) })
    }
    UpdateDialog(update, viewModel, onDownload = { onOpenUrl(viewModel.releasePage) })
}

/**
 * 새 버전 알림. 강제 업데이트는 닫을 수 없고 게임도 멈춘다 (데스크톱과 같다).
 * Android 앱은 스스로 설치할 수 없어 릴리스 페이지에서 APK를 받게 한다.
 */
@Composable
private fun UpdateDialog(state: UpdateState, viewModel: GameViewModel, onDownload: () -> Unit) {
    when (state) {
        is UpdateState.Available -> AlertDialog(
            onDismissRequest = viewModel::dismissUpdate,
            title = { Text("새 버전 ${state.version}") },
            text = { Text(state.message.ifBlank { "새 버전이 나왔습니다." }) },
            confirmButton = {
                TextButton(onClick = { onDownload(); viewModel.dismissUpdate() }) { Text("받으러 가기") }
            },
            dismissButton = {
                TextButton(onClick = viewModel::skipUpdate) { Text("이 버전 건너뛰기") }
                TextButton(onClick = viewModel::dismissUpdate) { Text("나중에") }
            },
        )
        is UpdateState.Required -> AlertDialog(
            onDismissRequest = {},
            properties = DialogProperties(dismissOnBackPress = false, dismissOnClickOutside = false),
            title = { Text("업데이트가 필요합니다") },
            text = {
                Text(
                    "이 버전은 더 이상 지원하지 않습니다. ${state.version}으로 업데이트해 주세요." +
                        state.message.let { if (it.isBlank()) "" else "\n\n$it" } +
                        "\n\n게임 세이브는 저장해 두었습니다.",
                )
            },
            confirmButton = { TextButton(onClick = onDownload) { Text("받으러 가기") } },
        )
        else -> {}
    }
}

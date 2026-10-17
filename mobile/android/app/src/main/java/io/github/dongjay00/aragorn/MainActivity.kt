package io.github.dongjay00.aragorn

import android.os.Bundle
import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import io.github.dongjay00.aragorn.core.GbButton

class MainActivity : ComponentActivity() {
    private val viewModel: GameViewModel by viewModels()

    private val openRom = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(viewModel::openRom)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        AndroidContext.init(applicationContext)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                // .gb/.gbc에는 등록된 MIME 타입이 없어 모든 파일을 보여 준다.
                GameScreen(viewModel, onOpenRom = { openRom.launch(arrayOf("*/*")) })
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

    // 블루투스·USB 게임패드와 키보드
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean =
        handleKey(keyCode, true) || super.onKeyDown(keyCode, event)

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean =
        handleKey(keyCode, false) || super.onKeyUp(keyCode, event)

    private fun handleKey(keyCode: Int, pressed: Boolean): Boolean {
        val button = KEY_MAP[keyCode] ?: return false
        viewModel.setButton(button, pressed)
        return true
    }

    private companion object {
        val KEY_MAP = mapOf(
            KeyEvent.KEYCODE_DPAD_UP to GbButton.UP,
            KeyEvent.KEYCODE_DPAD_DOWN to GbButton.DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT to GbButton.LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT to GbButton.RIGHT,
            // 게임패드: 데스크톱 기본값과 같다 (아래 버튼이 A, 오른쪽 버튼이 B)
            KeyEvent.KEYCODE_BUTTON_A to GbButton.A,
            KeyEvent.KEYCODE_BUTTON_B to GbButton.B,
            KeyEvent.KEYCODE_BUTTON_SELECT to GbButton.SELECT,
            KeyEvent.KEYCODE_BUTTON_START to GbButton.START,
            // 키보드: 데스크톱 기본 키와 같다
            KeyEvent.KEYCODE_X to GbButton.A,
            KeyEvent.KEYCODE_Z to GbButton.B,
            KeyEvent.KEYCODE_DEL to GbButton.SELECT,
            KeyEvent.KEYCODE_ENTER to GbButton.START,
        )
    }
}

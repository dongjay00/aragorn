package io.github.dongjay00.aragorn

import android.app.Application
import android.net.Uri
import android.provider.OpenableColumns
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.dongjay00.aragorn.core.Emulator
import io.github.dongjay00.aragorn.core.EmulatorException
import io.github.dongjay00.aragorn.core.GbButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * 실행 중인 게임 하나를 들고 있는다. 화면 회전이나 설정 변경에도 에뮬레이터는 그대로 돈다.
 * 에뮬레이션은 Rust 스레드에서 돌고, 여기서는 명령만 보낸다.
 */
class GameViewModel(application: Application) : AndroidViewModel(application) {
    private val _emulator = MutableStateFlow<Emulator?>(null)
    val emulator: StateFlow<Emulator?> = _emulator.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)
    /** 화면 아래에 보일 안내·오류 문구. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    /** 앱이 화면에 보이는지. 백그라운드에서는 일시정지한다. */
    private var visible = false

    /** 문서 선택기로 고른 ROM을 연다. 세이브는 앱 전용 폴더의 `saves/`에 둔다. */
    fun openRom(uri: Uri) {
        viewModelScope.launch {
            val opened = withContext(Dispatchers.IO) {
                runCatching {
                    val app = getApplication<Application>()
                    val rom = app.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                        ?: error("ROM 파일을 열 수 없습니다")
                    val name = displayName(uri)
                    val savePath = File(app.filesDir, "saves/$name")
                    // 같은 ROM을 다시 열면 새 세션이 세이브를 읽기 전에 지금 진행을 저장해 둔다.
                    _emulator.value?.flush()
                    Emulator(rom, savePath.path) to name
                }
            }
            opened
                .onSuccess { (emulator, name) ->
                    _emulator.value?.close()
                    _emulator.value = emulator
                    emulator.setPaused(!visible)
                    // 헤더에 제목이 없는 ROM(테스트 ROM 등)은 파일 이름을 보인다.
                    _notice.value = "${emulator.title().ifBlank { name }} 실행 중"
                }
                .onFailure { _notice.value = it.userMessage ?: "ROM을 열 수 없습니다" }
        }
    }

    fun setButton(button: GbButton, pressed: Boolean) {
        _emulator.value?.setButton(button, pressed)
    }

    fun saveState(slot: Int) = runSlot("슬롯 ${slot + 1}에 저장했습니다") {
        it.saveState(slot.toUByte())
    }

    fun loadState(slot: Int) = runSlot("슬롯 ${slot + 1}을(를) 불러왔습니다") {
        it.loadState(slot.toUByte())
    }

    /** 액티비티의 onStart/onStop에서 부른다. 일시정지할 때 Rust 쪽이 세이브를 저장한다. */
    fun setVisible(value: Boolean) {
        visible = value
        _emulator.value?.setPaused(!value)
    }

    /** 에뮬레이터가 쌓아 둔 오류 문구를 안내로 옮긴다. 화면 갱신마다 부른다. */
    fun collectErrors() {
        _emulator.value?.takeErrors()?.lastOrNull()?.let { _notice.value = it }
    }

    override fun onCleared() {
        _emulator.value?.close()
        _emulator.value = null
    }

    private fun runSlot(done: String, action: (Emulator) -> Unit) {
        val emulator = _emulator.value ?: return
        viewModelScope.launch {
            _notice.value = withContext(Dispatchers.IO) {
                try {
                    action(emulator)
                    done
                } catch (e: EmulatorException) {
                    e.userMessage
                }
            }
        }
    }

    /** 문서의 표시 이름(`Pokemon Crystal.gbc`). 세이브 파일 이름을 여기서 딴다. */
    private fun displayName(uri: Uri): String {
        val resolver = getApplication<Application>().contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
        // 경로 구분자가 들어가면 saves/ 밖에 쓰게 된다.
        return (name ?: uri.lastPathSegment ?: "rom").replace('/', '_')
    }
}

/** 사용자에게 보일 문구. 바인딩이 만든 `message`는 `reason=...` 형식이라 그대로 쓰지 않는다. */
private val Throwable.userMessage: String?
    get() = when (this) {
        is EmulatorException.Rom -> reason
        is EmulatorException.State -> reason
        is EmulatorException.Io -> reason
        is EmulatorException.Stopped -> "에뮬레이터가 이미 꺼졌습니다"
        else -> message
    }

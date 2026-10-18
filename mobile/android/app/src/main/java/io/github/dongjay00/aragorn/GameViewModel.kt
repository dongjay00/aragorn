package io.github.dongjay00.aragorn

import android.app.Application
import android.net.Uri
import android.os.SystemClock
import android.provider.OpenableColumns
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.dongjay00.aragorn.core.Emulator
import io.github.dongjay00.aragorn.core.EmulatorException
import io.github.dongjay00.aragorn.core.FastSpeed
import io.github.dongjay00.aragorn.core.GbButton
import io.github.dongjay00.aragorn.core.InputAction
import io.github.dongjay00.aragorn.core.InputDevice
import io.github.dongjay00.aragorn.core.Settings
import io.github.dongjay00.aragorn.core.SlotSummary
import io.github.dongjay00.aragorn.core.Speed
import io.github.dongjay00.aragorn.core.UpdateCheck
import io.github.dongjay00.aragorn.core.actionButton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** 앱 화면. 뒤로 가기는 게임 화면으로 돌아온다. */
enum class Page { GAME, SETTINGS, DEBUGGER }

/** 업데이트 확인 상태. */
sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data object UpToDate : UpdateState
    data class Failed(val reason: String) : UpdateState
    /** 나중에 해도 된다. 대화상자를 닫으면 [Idle]로 돌아간다. */
    data class Available(val version: String, val message: String) : UpdateState
    /** 업데이트 전까지 게임을 진행하지 않는다. */
    data class Required(val version: String, val message: String) : UpdateState
}

/**
 * 앱 상태 전체: 설정, 실행 중인 게임, 입력 처리, 업데이트 확인.
 * 화면 회전이나 설정 변경에도 에뮬레이터는 그대로 돈다. 에뮬레이션은 Rust 스레드에서 돌고 여기서는 명령만 보낸다.
 */
class GameViewModel(application: Application) : AndroidViewModel(application) {
    val settings: Settings = Settings.open(File(application.filesDir, "config.json").path)
    private val updates = UpdateChecker(BuildConfig.GITHUB_REPO)

    private val _emulator = MutableStateFlow<Emulator?>(null)
    val emulator: StateFlow<Emulator?> = _emulator.asStateFlow()

    private val _page = MutableStateFlow(Page.GAME)
    val page: StateFlow<Page> = _page.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)
    /** 화면 아래에 보일 안내·오류 문구. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    private val _muted = MutableStateFlow(settings.muted())
    val muted: StateFlow<Boolean> = _muted.asStateFlow()

    private val _paused = MutableStateFlow(false)
    /** 사용자가 일시정지했는지. */
    val paused: StateFlow<Boolean> = _paused.asStateFlow()

    private val _speedText = MutableStateFlow("")
    /** 지금 실행 배율 (`1배`, `4배`, `일시정지`). */
    val speedText: StateFlow<String> = _speedText.asStateFlow()

    private val _baseSpeed = MutableStateFlow(settings.baseSpeed())
    val baseSpeed: StateFlow<Speed> = _baseSpeed.asStateFlow()
    private val _fastSpeed = MutableStateFlow(settings.fastSpeed())
    val fastSpeed: StateFlow<FastSpeed> = _fastSpeed.asStateFlow()

    private val _bindingsVersion = MutableStateFlow(0)
    /** 키 설정이 바뀔 때마다 늘어난다. 설정 화면이 다시 읽는다. */
    val bindingsVersion: StateFlow<Int> = _bindingsVersion.asStateFlow()

    private val _capturing = MutableStateFlow<Pair<InputDevice, InputAction>?>(null)
    /** 키 설정에서 입력을 기다리는 칸. */
    val capturing: StateFlow<Pair<InputDevice, InputAction>?> = _capturing.asStateFlow()

    private val _slots = MutableStateFlow<List<SlotSummary>>(emptyList())
    val slots: StateFlow<List<SlotSummary>> = _slots.asStateFlow()

    private val _breakpoints = MutableStateFlow<List<UShort>>(emptyList())
    /** 디버거 브레이크포인트. ROM을 바꿔도 유지한다 (데스크톱과 같다). */
    val breakpoints: StateFlow<List<UShort>> = _breakpoints.asStateFlow()

    private val _channelMuted = MutableStateFlow(List(4) { false })
    /** 디버거의 채널별 음소거. ROM을 바꿔도 유지한다. */
    val channelMuted: StateFlow<List<Boolean>> = _channelMuted.asStateFlow()

    private val _update = MutableStateFlow<UpdateState>(UpdateState.Idle)
    val update: StateFlow<UpdateState> = _update.asStateFlow()
    val updateEnabled get() = updates.enabled
    val releasePage get() = updates.releasePage

    /** 앱이 화면에 보이는지. 백그라운드에서는 일시정지한다. */
    private var visible = false
    /** 터치 배속 버튼과 배속 키. 둘 중 하나라도 누르면 배속이다. */
    private var fastTouch = false
    private var fastKey = false

    /** 마지막으로 자동 확인한 시각 (elapsedRealtime). */
    private var lastAutoCheck: Long? = null

    init {
        // 데스크톱처럼 6시간마다 확인한다. 화면에 보일 때만 확인한다.
        viewModelScope.launch {
            while (true) {
                delay(RECHECK_INTERVAL_MS)
                if (visible) autoCheck()
            }
        }
    }

    // ---- ROM ----

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
                    configure(emulator)
                    _emulator.value = emulator
                    // 헤더에 제목이 없는 ROM(테스트 ROM 등)은 파일 이름을 보인다.
                    _notice.value = "${emulator.title().ifBlank { name }} 실행 중"
                    refreshSlots()
                }
                .onFailure { _notice.value = it.userMessage ?: "ROM을 열 수 없습니다" }
        }
    }

    /** 새 에뮬레이터에 지금 설정과 디버거 상태를 적용한다. */
    private fun configure(emulator: Emulator) {
        emulator.setSpeed(_baseSpeed.value, _fastSpeed.value)
        emulator.setMuted(_muted.value)
        emulator.setFastHeld(fastTouch || fastKey)
        emulator.setBreakpoints(_breakpoints.value)
        _channelMuted.value.forEachIndexed { channel, muted ->
            emulator.setChannelMuted(channel.toUByte(), muted)
        }
        emulator.setBlocked(_update.value is UpdateState.Required)
        emulator.setBackground(!visible)
        _paused.value = false
    }

    // ---- 화면 ----

    fun showPage(page: Page) {
        _capturing.value = null
        _page.value = page
    }

    /** 액티비티의 onStart/onStop에서 부른다. 백그라운드로 가면 Rust 쪽이 세이브를 저장한다. */
    fun setVisible(value: Boolean) {
        visible = value
        _emulator.value?.setBackground(!value)
        // 앱을 처음 열 때, 그리고 오래 백그라운드에 있다가 돌아왔을 때 확인한다.
        val last = lastAutoCheck
        if (value && (last == null || SystemClock.elapsedRealtime() - last >= RECHECK_INTERVAL_MS)) {
            autoCheck()
        }
    }

    private fun autoCheck() {
        lastAutoCheck = SystemClock.elapsedRealtime()
        checkForUpdate(manual = false)
    }

    /** 화면 갱신마다 부른다. 에뮬레이터의 안내 문구와 실행 상태를 옮겨 온다. */
    fun poll() {
        val emulator = _emulator.value ?: return
        emulator.takeNotices().lastOrNull()?.let { _notice.value = it }
        _speedText.value = emulator.speedText()
        _paused.value = emulator.isPaused()
    }

    // ---- 입력 ----

    /** 화면의 가상 패드. */
    fun setButton(button: GbButton, pressed: Boolean) {
        _emulator.value?.setButton(button, pressed)
    }

    /** 화면의 배속 버튼 (누르는 동안). */
    fun setFastTouch(held: Boolean) {
        fastTouch = held
        _emulator.value?.setFastHeld(fastTouch || fastKey)
    }

    /**
     * 키보드·게임패드 입력. 키 설정 중이면 그 칸에 지정하고, 아니면 지정된 기능을 실행한다.
     * 처리했으면 `true`. `repeat`는 키를 누르고 있어 반복된 입력이다.
     */
    fun onKey(device: InputDevice, name: String, pressed: Boolean, repeat: Boolean = false): Boolean {
        _capturing.value?.let { (captureDevice, action) ->
            if (pressed && !repeat && device == captureDevice) bind(device, action, name)
            return true
        }
        if (_page.value != Page.GAME) return false
        val actions = settings.actionsFor(device, name)
        for (action in actions) {
            val button = actionButton(action)
            when {
                button != null -> setButton(button, pressed)
                action == InputAction.FAST_FORWARD -> {
                    fastKey = pressed
                    _emulator.value?.setFastHeld(fastTouch || fastKey)
                }
                !pressed || repeat -> {}
                action == InputAction.PAUSE -> togglePause()
                action == InputAction.MUTE -> setMuted(!_muted.value)
            }
        }
        return actions.isNotEmpty()
    }

    /** 키보드 F1–F10: 스테이트 저장, Shift를 함께 누르면 불러오기 (데스크톱과 같다). */
    fun onSlotKey(slot: Int, load: Boolean) {
        if (_capturing.value != null) {
            _notice.value = "F1–F10은 스테이트 단축키라 지정할 수 없습니다"
            return
        }
        if (_page.value != Page.GAME) return
        if (load) loadState(slot) else saveState(slot)
    }

    // ---- 실행 ----

    fun togglePause() {
        val emulator = _emulator.value ?: return
        _paused.value = emulator.togglePause()
    }

    fun setMuted(muted: Boolean) {
        _muted.value = muted
        _emulator.value?.setMuted(muted)
        saveSetting { settings.setMuted(muted) }
    }

    fun setBaseSpeed(speed: Speed) {
        _baseSpeed.value = speed
        _emulator.value?.setSpeed(speed, _fastSpeed.value)
        saveSetting { settings.setBaseSpeed(speed) }
    }

    fun setFastSpeed(speed: FastSpeed) {
        _fastSpeed.value = speed
        _emulator.value?.setSpeed(_baseSpeed.value, speed)
        saveSetting { settings.setFastSpeed(speed) }
    }

    // ---- 스테이트 ----

    fun saveState(slot: Int) = runSlot("슬롯 ${slot + 1}에 저장했습니다") {
        it.saveState(slot.toUByte())
    }

    fun loadState(slot: Int) = runSlot("슬롯 ${slot + 1}을(를) 불러왔습니다") {
        it.loadState(slot.toUByte())
    }

    /** 슬롯 목록(썸네일, 저장 시각)을 다시 읽는다. 슬롯 화면을 열 때 부른다. */
    fun refreshSlots() {
        val emulator = _emulator.value ?: run {
            _slots.value = emptyList()
            return
        }
        viewModelScope.launch {
            _slots.value = withContext(Dispatchers.IO) { emulator.slots() }
        }
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
            refreshSlots()
        }
    }

    // ---- 키 설정 ----

    /** 칸을 누르면 입력을 기다리고, 같은 칸을 다시 누르면 그만둔다. */
    fun toggleCapture(device: InputDevice, action: InputAction) {
        _capturing.update { if (it == device to action) null else device to action }
    }

    fun cancelCapture() {
        _capturing.value = null
    }

    /** 입력을 기다리는 칸의 지정을 지운다. */
    fun clearCaptured() {
        val (device, action) = _capturing.value ?: return
        bind(device, action, "")
    }

    fun resetInput() {
        _capturing.value = null
        saveSetting { settings.resetInput() }
        _baseSpeed.value = settings.baseSpeed()
        _fastSpeed.value = settings.fastSpeed()
        _emulator.value?.setSpeed(_baseSpeed.value, _fastSpeed.value)
        _bindingsVersion.update { it + 1 }
    }

    private fun bind(device: InputDevice, action: InputAction, name: String) {
        if (name.isNotEmpty() && device == InputDevice.KEYBOARD &&
            GameInput.SLOT_KEYS.any { GameInput.keyName(it) == name }
        ) {
            _notice.value = "F1–F10은 스테이트 단축키라 지정할 수 없습니다"
            return
        }
        _capturing.value = null
        saveSetting { settings.bind(device, action, name) }
        _bindingsVersion.update { it + 1 }
    }

    private fun saveSetting(change: () -> Unit) {
        try {
            change()
        } catch (e: EmulatorException) {
            _notice.value = e.userMessage
        }
    }

    // ---- 디버거 ----

    fun toggleBreakpoint(pc: UShort) {
        _breakpoints.update { list ->
            (if (pc in list) list - pc else list + pc).sorted()
        }
        _emulator.value?.setBreakpoints(_breakpoints.value)
    }

    fun setChannelMuted(channel: Int, muted: Boolean) {
        _channelMuted.update { list -> list.toMutableList().also { it[channel] = muted } }
        _emulator.value?.setChannelMuted(channel.toUByte(), muted)
    }

    // ---- 업데이트 ----

    /** `manual`이면 사용자가 누른 것이라 결과를 안내 문구로도 보인다. */
    fun checkForUpdate(manual: Boolean) {
        if (!updates.enabled) {
            if (manual) _notice.value = "이 빌드는 업데이트를 확인하지 않습니다"
            return
        }
        val current = _update.value
        if (current is UpdateState.Checking || current is UpdateState.Required) return
        _update.value = UpdateState.Checking
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { updates.check(settings.skippedVersion()) }
            }
            _update.value = result.fold(
                onSuccess = { check ->
                    when (check) {
                        is UpdateCheck.UpToDate -> UpdateState.UpToDate
                        is UpdateCheck.Available -> UpdateState.Available(check.version, check.message)
                        is UpdateCheck.Required -> UpdateState.Required(check.version, check.message)
                    }
                },
                onFailure = { UpdateState.Failed(it.userMessage ?: "업데이트를 확인하지 못했습니다") },
            )
            val state = _update.value
            if (state is UpdateState.Required) _emulator.value?.setBlocked(true)
            if (manual) {
                when (state) {
                    UpdateState.UpToDate -> _notice.value = "최신 버전입니다"
                    is UpdateState.Failed -> _notice.value = state.reason
                    else -> {}
                }
            }
        }
    }

    /** "나중에": 다음 확인 때 다시 알린다. */
    fun dismissUpdate() {
        if (_update.value is UpdateState.Available) _update.value = UpdateState.Idle
    }

    /** "이 버전 건너뛰기": 이 버전은 다시 알리지 않는다. */
    fun skipUpdate() {
        val state = _update.value as? UpdateState.Available ?: return
        saveSetting { settings.setSkippedVersion(state.version) }
        _update.value = UpdateState.Idle
    }

    override fun onCleared() {
        _emulator.value?.close()
        _emulator.value = null
        settings.close()
    }

    /** 문서의 표시 이름(`Pokemon Crystal.gbc`). 세이브 파일 이름을 여기서 딴다. */
    private fun displayName(uri: Uri): String {
        val resolver = getApplication<Application>().contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
        // 경로 구분자가 들어가면 saves/ 밖에 쓰게 된다.
        return (name ?: uri.lastPathSegment ?: "rom").replace('/', '_')
    }

    private companion object {
        const val RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000L
    }
}

/** 사용자에게 보일 문구. 바인딩이 만든 `message`는 `reason=...` 형식이라 그대로 쓰지 않는다. */
val Throwable.userMessage: String?
    get() = when (this) {
        is EmulatorException.Rom -> reason
        is EmulatorException.State -> reason
        is EmulatorException.Io -> reason
        is EmulatorException.Update -> reason
        is EmulatorException.Stopped -> "에뮬레이터가 이미 꺼졌습니다"
        else -> message
    }

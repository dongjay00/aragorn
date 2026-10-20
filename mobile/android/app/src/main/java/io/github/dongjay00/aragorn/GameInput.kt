package io.github.dongjay00.aragorn

import android.view.InputDevice as AndroidInputDevice
import android.view.InputEvent
import android.view.KeyEvent
import android.view.MotionEvent
import io.github.dongjay00.aragorn.core.InputDevice

/**
 * 키보드·게임패드 입력을 키 이름으로 바꾼다. 이름은 `KeyEvent.keyCodeToString`에서 `KEYCODE_`를 뗀 것이고
 * (`DPAD_UP`, `BUTTON_A`, `X`), 설정 파일에 그대로 저장된다.
 *
 * 많은 게임패드가 방향키와 트리거를 버튼이 아니라 축(hat, trigger)으로 보낸다. 축은 같은 이름의 버튼으로
 * 바꿔 버튼과 똑같이 다룬다. 왼쪽 아날로그 스틱도 방향키(`DPAD_*`)로 다룬다.
 */
object GameInput {
    /** 데스크톱처럼 F1–F10은 스테이트 단축키라 키 설정에 쓸 수 없다. */
    val SLOT_KEYS = (KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F10).toList()

    /** 시스템 탐색에 쓰는 키. 게임에 넘기지 않는다. */
    private val SYSTEM_KEYS = setOf(
        KeyEvent.KEYCODE_BACK,
        KeyEvent.KEYCODE_HOME,
        KeyEvent.KEYCODE_VOLUME_UP,
        KeyEvent.KEYCODE_VOLUME_DOWN,
        KeyEvent.KEYCODE_VOLUME_MUTE,
        KeyEvent.KEYCODE_POWER,
    )

    fun isSystemKey(keyCode: Int) = keyCode in SYSTEM_KEYS

    fun keyName(keyCode: Int): String = KeyEvent.keyCodeToString(keyCode).removePrefix("KEYCODE_")

    /** 입력이 온 장치. 게임패드 버튼도 키보드 소스를 함께 달고 오므로 게임패드를 먼저 본다. */
    fun deviceOf(event: InputEvent): InputDevice? {
        val sources = event.source or (event.device?.sources ?: 0)
        return when {
            sources.has(AndroidInputDevice.SOURCE_GAMEPAD) ||
                sources.has(AndroidInputDevice.SOURCE_JOYSTICK) -> InputDevice.GAMEPAD
            sources.has(AndroidInputDevice.SOURCE_KEYBOARD) -> InputDevice.KEYBOARD
            else -> null
        }
    }

    private fun Int.has(source: Int) = this and source == source
}

/**
 * 게임패드 축을 버튼처럼 바꾼다. 축 값이 바뀔 때마다 눌리거나 떼어진 이름을 알려 준다.
 * 게임패드마다 하나씩 둔다.
 */
class AxisButtons {
    private val pressed = mutableSetOf<String>()

    /** `onChange(name, pressed)`를 바뀐 이름마다 부른다. */
    fun update(event: MotionEvent, onChange: (String, Boolean) -> Unit) {
        val hatX = event.getAxisValue(MotionEvent.AXIS_HAT_X)
        val hatY = event.getAxisValue(MotionEvent.AXIS_HAT_Y)
        val left = maxOf(
            event.getAxisValue(MotionEvent.AXIS_LTRIGGER),
            event.getAxisValue(MotionEvent.AXIS_BRAKE),
        )
        val right = maxOf(
            event.getAxisValue(MotionEvent.AXIS_RTRIGGER),
            event.getAxisValue(MotionEvent.AXIS_GAS),
        )
        val stickX = event.getAxisValue(MotionEvent.AXIS_X)
        val stickY = event.getAxisValue(MotionEvent.AXIS_Y)
        val now = buildSet {
            if (hatX < -HALF || stickX < -HALF) add("DPAD_LEFT")
            if (hatX > HALF || stickX > HALF) add("DPAD_RIGHT")
            if (hatY < -HALF || stickY < -HALF) add("DPAD_UP")
            if (hatY > HALF || stickY > HALF) add("DPAD_DOWN")
            if (left > HALF) add("BUTTON_L2")
            if (right > HALF) add("BUTTON_R2")
        }
        (pressed - now).forEach { onChange(it, false) }
        (now - pressed).forEach { onChange(it, true) }
        pressed.clear()
        pressed.addAll(now)
    }

    private companion object {
        const val HALF = 0.5f
    }
}

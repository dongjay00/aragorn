package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel
import io.github.dongjay00.aragorn.Page
import io.github.dongjay00.aragorn.core.GbButton
import kotlin.math.abs

private val Body = Color(0xFF1E1E24)
private val KeyColor = Color(0xFF3A3A44)
private val KeyPressed = Color(0xFF5A5A68)
private val ActionColor = Color(0xFF8B2252)
/** 가상 패드를 누를 때 햅틱을 낼지 (설정의 "버튼 햅틱"). */
private val LocalPadHaptics = compositionLocalOf { true }

private val PillShape = RoundedCornerShape(50)
private val KeyShape = RoundedCornerShape(8.dp)
/** 가로 화면에서 게임 화면 위아래에 쓰는 높이 (툴바 + 상태 줄). */
private val LANDSCAPE_CHROME = 96.dp

@Composable
fun GameScreen(viewModel: GameViewModel, onOpenRom: () -> Unit) {
    val emulator by viewModel.emulator.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    val paused by viewModel.paused.collectAsStateWithLifecycle()
    val muted by viewModel.muted.collectAsStateWithLifecycle()
    val speedText by viewModel.speedText.collectAsStateWithLifecycle()
    val haptics by viewModel.haptics.collectAsStateWithLifecycle()
    var slotsOpen by remember { mutableStateOf(false) }

    val toolbar = @Composable {
        Toolbar(
            hasGame = emulator != null,
            paused = paused,
            muted = muted,
            onOpenRom = onOpenRom,
            onSlots = {
                viewModel.refreshSlots()
                slotsOpen = true
            },
            onPause = viewModel::togglePause,
            onMute = { viewModel.setMuted(!muted) },
            onSettings = { viewModel.showPage(Page.SETTINGS) },
            onDebugger = { viewModel.showPage(Page.DEBUGGER) },
            onCheckUpdate = { viewModel.checkForUpdate(manual = true) },
        )
    }
    val screen = @Composable { modifier: Modifier ->
        val current = emulator
        if (current != null) {
            LcdScreen(current, modifier, onFrame = viewModel::poll)
        } else {
            Box(
                modifier.aspectRatio(160f / 144f).background(Color.Black),
                contentAlignment = Alignment.Center,
            ) {
                Text("ROM을 열어 주세요", color = Color.Gray)
            }
        }
    }
    val status = @Composable {
        Row(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
            Text(
                notice ?: "",
                color = Color.LightGray,
                fontSize = 12.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (emulator != null) Text(speedText, color = Color.LightGray, fontSize = 12.sp)
        }
    }

    CompositionLocalProvider(LocalPadHaptics provides haptics) {
        BoxWithConstraints(Modifier.fillMaxSize().background(Body).safeDrawingPadding()) {
            if (maxWidth > maxHeight) {
                // 가로: 왼쪽 방향키, 가운데 화면, 오른쪽 A/B. 화면 폭은 남는 높이(툴바와 상태 줄을 뺀 것)로 정한다.
                val lcdWidth = (maxHeight - LANDSCAPE_CHROME) * (160f / 144f)
                Column(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
                    toolbar()
                    Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                        Column(
                            Modifier.weight(1f).fillMaxHeight(),
                            verticalArrangement = Arrangement.SpaceEvenly,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            DPad(viewModel::setButton)
                            PadButton("SELECT", GbButton.SELECT, viewModel::setButton, PillShape, 72.dp, 28.dp)
                        }
                        Column(Modifier.width(lcdWidth).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
                            screen(Modifier.fillMaxWidth())
                            status()
                        }
                        Column(
                            Modifier.weight(1f).fillMaxHeight(),
                            verticalArrangement = Arrangement.SpaceEvenly,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            ActionButtons(viewModel)
                            PadButton("START", GbButton.START, viewModel::setButton, PillShape, 72.dp, 28.dp)
                        }
                    }
                }
            } else {
                Column(
                    Modifier.fillMaxSize().padding(horizontal = 16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    toolbar()
                    screen(Modifier.fillMaxWidth())
                    status()
                    Spacer(Modifier.weight(1f))
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        DPad(viewModel::setButton)
                        ActionButtons(viewModel)
                    }
                    Spacer(Modifier.height(24.dp))
                    Row(Modifier.padding(bottom = 24.dp)) {
                        PadButton("SELECT", GbButton.SELECT, viewModel::setButton, PillShape, 72.dp, 28.dp)
                        Spacer(Modifier.width(16.dp))
                        PadButton("START", GbButton.START, viewModel::setButton, PillShape, 72.dp, 28.dp)
                    }
                }
            }
        }

    }

    if (slotsOpen) {
        SlotsSheet(viewModel, onDismiss = { slotsOpen = false })
    }
}

@Composable
private fun Toolbar(
    hasGame: Boolean,
    paused: Boolean,
    muted: Boolean,
    onOpenRom: () -> Unit,
    onSlots: () -> Unit,
    onPause: () -> Unit,
    onMute: () -> Unit,
    onSettings: () -> Unit,
    onDebugger: () -> Unit,
    onCheckUpdate: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = onOpenRom) { Text("ROM") }
        TextButton(onClick = onSlots, enabled = hasGame) { Text("스테이트") }
        TextButton(onClick = onPause, enabled = hasGame) { Text(if (paused) "계속" else "일시정지") }
        TextButton(onClick = onMute) { Text(if (muted) "소리 켜기" else "음소거") }
        Spacer(Modifier.weight(1f))
        Box {
            TextButton(onClick = { menuOpen = true }) { Text("⋮", fontSize = 18.sp) }
            DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                DropdownMenuItem(text = { Text("설정") }, onClick = { menuOpen = false; onSettings() })
                DropdownMenuItem(
                    text = { Text("디버거") },
                    enabled = hasGame,
                    onClick = { menuOpen = false; onDebugger() },
                )
                DropdownMenuItem(
                    text = { Text("업데이트 확인") },
                    onClick = { menuOpen = false; onCheckUpdate() },
                )
            }
        }
    }
}

/** A/B와 배속 버튼. 배속은 누르는 동안만 빨라진다 (데스크톱 배속 키와 같다). */
@Composable
private fun ActionButtons(viewModel: GameViewModel) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            PadButton("B", GbButton.B, viewModel::setButton, modifier = Modifier.padding(top = 32.dp))
            Spacer(Modifier.width(16.dp))
            PadButton("A", GbButton.A, viewModel::setButton, modifier = Modifier.padding(bottom = 32.dp))
        }
        Spacer(Modifier.height(12.dp))
        HoldButton("▶▶ 배속", onHold = viewModel::setFastTouch, shape = PillShape, width = 88.dp, height = 28.dp)
    }
}

/**
 * 방향키. 손가락을 떼지 않고 미끄러뜨려 방향을 바꿀 수 있고, 대각선도 누를 수 있다.
 * 가운데에서 가까우면(지름의 약 1/5) 아무 방향도 누르지 않는다.
 */
@Composable
private fun DPad(onButton: (GbButton, Boolean) -> Unit) {
    val key = 52.dp
    val haptics = LocalHapticFeedback.current
    val hapticsOn by rememberUpdatedState(LocalPadHaptics.current)
    var pressed by remember { mutableStateOf(emptySet<GbButton>()) }
    fun press(now: Set<GbButton>) {
        if (now == pressed) return
        (pressed - now).forEach { onButton(it, false) }
        (now - pressed).forEach { onButton(it, true) }
        if (hapticsOn && (now - pressed).isNotEmpty()) {
            haptics.performHapticFeedback(HapticFeedbackType.VirtualKey)
        }
        pressed = now
    }
    Box(
        Modifier
            .size(key * 3)
            .pointerInput(Unit) {
                val center = Offset(size.width / 2f, size.height / 2f)
                // 대각선 판정 기준: 가운데 칸 절반보다 조금 작게
                val threshold = size.width / 6f * 0.75f
                fun directions(position: Offset): Set<GbButton> {
                    val d = position - center
                    if (abs(d.x) < threshold && abs(d.y) < threshold) return emptySet()
                    return buildSet {
                        // 한 축이 다른 축의 절반도 안 되면 그 축은 누르지 않는다 (의도치 않은 대각선 방지).
                        if (abs(d.x) >= threshold && abs(d.x) * 2 >= abs(d.y)) {
                            add(if (d.x > 0) GbButton.RIGHT else GbButton.LEFT)
                        }
                        if (abs(d.y) >= threshold && abs(d.y) * 2 >= abs(d.x)) {
                            add(if (d.y > 0) GbButton.DOWN else GbButton.UP)
                        }
                    }
                }
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    press(directions(down.position))
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) break
                            if (change.positionChange() != Offset.Zero) press(directions(change.position))
                        }
                    } finally {
                        press(emptySet())
                    }
                }
            },
    ) {
        val shown = @Composable { label: String, button: GbButton, modifier: Modifier ->
            Box(
                modifier
                    .size(key)
                    .clip(KeyShape)
                    .background(if (button in pressed) KeyPressed else KeyColor),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, color = Color.White, style = MaterialTheme.typography.labelMedium)
            }
        }
        shown("▲", GbButton.UP, Modifier.align(Alignment.TopCenter))
        shown("◀", GbButton.LEFT, Modifier.align(Alignment.CenterStart))
        shown("▶", GbButton.RIGHT, Modifier.align(Alignment.CenterEnd))
        shown("▼", GbButton.DOWN, Modifier.align(Alignment.BottomCenter))
    }
}

/** 누르는 동안 눌린 게임보이 버튼. 버튼마다 따로 터치를 받으므로 방향키와 함께 누를 수 있다. */
@Composable
private fun PadButton(
    label: String,
    button: GbButton,
    onButton: (GbButton, Boolean) -> Unit,
    shape: Shape = CircleShape,
    width: Dp = 64.dp,
    height: Dp = width,
    modifier: Modifier = Modifier,
) {
    val color = if (button == GbButton.A || button == GbButton.B) ActionColor else KeyColor
    HoldButton(label, { onButton(button, it) }, shape, width, height, modifier, color)
}

@Composable
private fun HoldButton(
    label: String,
    onHold: (Boolean) -> Unit,
    shape: Shape,
    width: Dp,
    height: Dp,
    modifier: Modifier = Modifier,
    color: Color = KeyColor,
) {
    val haptics = LocalHapticFeedback.current
    val hapticsOn by rememberUpdatedState(LocalPadHaptics.current)
    var held by remember { mutableStateOf(false) }
    Box(
        modifier
            .size(width, height)
            .clip(shape)
            .background(if (held) color.copy(alpha = 0.7f) else color)
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    held = true
                    onHold(true)
                    if (hapticsOn) haptics.performHapticFeedback(HapticFeedbackType.VirtualKey)
                    try {
                        waitForUpOrCancellation()
                    } finally {
                        held = false
                        onHold(false)
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Color.White, style = MaterialTheme.typography.labelMedium)
    }
}

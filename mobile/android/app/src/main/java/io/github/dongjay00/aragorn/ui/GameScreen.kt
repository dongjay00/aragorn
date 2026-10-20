package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
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
import androidx.compose.foundation.layout.offset
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel
import io.github.dongjay00.aragorn.Page
import io.github.dongjay00.aragorn.PadGroup
import io.github.dongjay00.aragorn.core.GbButton
import io.github.dongjay00.aragorn.core.PadLayout
import io.github.dongjay00.aragorn.core.RecentRom
import kotlin.math.abs

private val Body = Color(0xFF1E1E24)
private val KeyColor = Color(0xFF3A3A44)
private val KeyPressed = Color(0xFF5A5A68)
private val ActionColor = Color(0xFF8B2252)
private val EditColor = Color(0xFFFFD54F)
/** 가상 패드를 누를 때 햅틱을 낼지 (설정의 "버튼 햅틱"). */
private val LocalPadHaptics = compositionLocalOf { true }
/** 가상 패드 버튼 크기 배율 (설정의 "가상 패드"). */
private val LocalPadScale = compositionLocalOf { 1f }
/** 패드를 아래에서 띄우는 최대 높이. 설정의 높이(0–1)에 곱한다. */
private val MAX_LIFT = 160.dp
/** 배율 1에서 방향키와 A/B를 나란히 놓는 데 필요한 폭 (세로 화면). */
private val PORTRAIT_PAD_WIDTH = 320.dp
/** 배율 1에서 가로 화면 한쪽 칸에 필요한 폭과 높이 (방향키와 SELECT, 또는 A/B와 START). */
private val LANDSCAPE_SIDE_WIDTH = 170.dp
private val LANDSCAPE_SIDE_HEIGHT = 230.dp
/** 툴바 ROM 메뉴에 보일 최근 게임 수. */
private const val MENU_RECENT = 5

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
    val pad by viewModel.padLayout.collectAsStateWithLifecycle()
    val video by viewModel.video.collectAsStateWithLifecycle()
    val recent by viewModel.recent.collectAsStateWithLifecycle()
    val rewindEnabled by viewModel.rewindEnabled.collectAsStateWithLifecycle()
    val editing by viewModel.editingPad.collectAsStateWithLifecycle()
    val linkText by viewModel.linkText.collectAsStateWithLifecycle()
    var slotsOpen by remember { mutableStateOf(false) }

    val editBar = @Composable { landscape: Boolean ->
        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                "버튼 묶음을 끌어 옮기세요",
                color = Color.White,
                modifier = Modifier.weight(1f).padding(start = 12.dp),
            )
            TextButton(onClick = { viewModel.resetPadPositions(landscape) }) { Text("초기화") }
            TextButton(onClick = viewModel::finishPadEdit) { Text("완료") }
        }
    }
    val toolbar = @Composable {
        Toolbar(
            hasGame = emulator != null,
            paused = paused,
            muted = muted,
            recent = recent.take(MENU_RECENT),
            onOpenRom = onOpenRom,
            onOpenRecent = viewModel::openRecent,
            onSlots = {
                viewModel.refreshSlots()
                slotsOpen = true
            },
            onPause = viewModel::togglePause,
            onMute = { viewModel.setMuted(!muted) },
            onSettings = { viewModel.showPage(Page.SETTINGS) },
            onDebugger = { viewModel.showPage(Page.DEBUGGER) },
            onCheats = { viewModel.showPage(Page.CHEATS) },
            onScreenshot = viewModel::saveScreenshot,
            onRestart = viewModel::restart,
            onEditPad = viewModel::startPadEdit,
            onLink = { viewModel.showPage(Page.LINK) },
            onCheckUpdate = { viewModel.checkForUpdate(manual = true) },
        )
    }
    val screen = @Composable { modifier: Modifier ->
        val current = emulator
        if (current != null) {
            LcdScreen(current, modifier, grid = video.lcdGrid, onFrame = viewModel::poll)
        } else {
            EmptyScreen(modifier, recent, onOpenRecent = viewModel::openRecent)
        }
    }
    val status = @Composable {
        Row(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
            Text(
                // 링크를 쓰는 동안에는 링크 상태를 먼저 보인다.
                linkText ?: notice ?: "",
                color = Color.LightGray,
                fontSize = 12.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (emulator != null) Text(speedText, color = Color.LightGray, fontSize = 12.sp)
        }
    }

    val lift = MAX_LIFT * pad.lift
    val actions = @Composable { ActionButtons(viewModel, rewindEnabled) }
    BoxWithConstraints(Modifier.fillMaxSize().background(Body).safeDrawingPadding()) {
        val landscape = maxWidth > maxHeight
        // 가로: 왼쪽 방향키, 가운데 화면, 오른쪽 A/B. 화면 폭은 남는 높이(툴바와 상태 줄을 뺀 것)로 정한다.
        val lcdWidth = (maxHeight - LANDSCAPE_CHROME) * (160f / 144f)
        // 설정한 크기가 화면 폭에 들어가지 않으면 들어가는 만큼만 키운다.
        val fit = if (landscape) {
            minOf(
                (maxWidth - 32.dp - lcdWidth) / 2 / LANDSCAPE_SIDE_WIDTH,
                (maxHeight - LANDSCAPE_CHROME / 2 - lift) / LANDSCAPE_SIDE_HEIGHT,
            )
        } else {
            (maxWidth - 32.dp) / PORTRAIT_PAD_WIDTH
        }
        CompositionLocalProvider(
            LocalPadHaptics provides haptics,
            LocalPadScale provides minOf(pad.scale, maxOf(fit, 0.5f)),
        ) {
            val offsets = if (landscape) pad.landscape else pad.portrait
            val movable = @Composable { group: PadGroup, content: @Composable () -> Unit ->
                val (x, y) = when (group) {
                    PadGroup.DPAD -> offsets.dpadX to offsets.dpadY
                    PadGroup.BUTTONS -> offsets.buttonsX to offsets.buttonsY
                    PadGroup.SELECT -> offsets.selectX to offsets.selectY
                    PadGroup.START -> offsets.startX to offsets.startY
                }
                Movable(x, y, pad.opacity, editing, onDrag = { dx, dy -> viewModel.movePad(landscape, group, dx, dy) }, content)
            }
            if (landscape) {
                Column(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
                    if (editing) editBar(true) else toolbar()
                    Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                        Column(
                            Modifier.weight(1f).fillMaxHeight().padding(bottom = lift),
                            verticalArrangement = Arrangement.SpaceEvenly,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            movable(PadGroup.DPAD) { DPad(viewModel::setButton) }
                            movable(PadGroup.SELECT) {
                                PadButton("SELECT", GbButton.SELECT, viewModel::setButton, PillShape, 72.dp, 28.dp)
                            }
                        }
                        Column(Modifier.width(lcdWidth).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
                            screen(Modifier.fillMaxWidth())
                            status()
                        }
                        Column(
                            Modifier.weight(1f).fillMaxHeight().padding(bottom = lift),
                            verticalArrangement = Arrangement.SpaceEvenly,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            movable(PadGroup.BUTTONS) { actions() }
                            movable(PadGroup.START) {
                                PadButton("START", GbButton.START, viewModel::setButton, PillShape, 72.dp, 28.dp)
                            }
                        }
                    }
                }
            } else {
                Column(
                    Modifier.fillMaxSize().padding(horizontal = 16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    if (editing) editBar(false) else toolbar()
                    screen(Modifier.fillMaxWidth())
                    status()
                    Spacer(Modifier.weight(1f))
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        movable(PadGroup.DPAD) { DPad(viewModel::setButton) }
                        movable(PadGroup.BUTTONS) { actions() }
                    }
                    Spacer(Modifier.height(24.dp))
                    Row(Modifier.padding(bottom = 24.dp + lift)) {
                        movable(PadGroup.SELECT) {
                            PadButton("SELECT", GbButton.SELECT, viewModel::setButton, PillShape, 72.dp, 28.dp)
                        }
                        Spacer(Modifier.width(16.dp))
                        movable(PadGroup.START) {
                            PadButton("START", GbButton.START, viewModel::setButton, PillShape, 72.dp, 28.dp)
                        }
                    }
                }
            }
        }

    }

    if (slotsOpen) {
        SlotsSheet(viewModel, onDismiss = { slotsOpen = false })
    }
}

/**
 * 기본 자리에서 (`x`, `y`) dp만큼 옮겨 그린 버튼 묶음. 편집 중에는 위에 덮개를 씌워 버튼 대신 끌기를 받는다.
 */
@Composable
private fun Movable(
    x: Float,
    y: Float,
    opacity: Float,
    editing: Boolean,
    onDrag: (Float, Float) -> Unit,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current.density
    val drag by rememberUpdatedState(onDrag)
    // 불투명도는 묶음마다 준다. 큰 묶음에 주면 그 경계 밖으로 옮긴 버튼이 잘린다.
    Box(Modifier.offset(x.dp, y.dp)) {
        Box(Modifier.alpha(opacity)) { content() }
        if (editing) {
            Box(
                Modifier
                    .matchParentSize()
                    .border(2.dp, EditColor, KeyShape)
                    .background(EditColor.copy(alpha = 0.15f), KeyShape)
                    .pointerInput(Unit) {
                        detectDragGestures { change, amount ->
                            change.consume()
                            drag(amount.x / density, amount.y / density)
                        }
                    },
            )
        }
    }
}

/** 게임이 없을 때 화면 자리. 최근 게임을 누르면 바로 연다. */
@Composable
private fun EmptyScreen(modifier: Modifier, recent: List<RecentRom>, onOpenRecent: (String) -> Unit) {
    Column(
        modifier.aspectRatio(160f / 144f).background(Color.Black).padding(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        if (recent.isEmpty()) {
            Text("ROM을 열어 주세요", color = Color.Gray)
            return@Column
        }
        Text("최근 게임", color = Color.Gray, style = MaterialTheme.typography.labelMedium)
        Spacer(Modifier.height(4.dp))
        for (game in recent.take(MENU_RECENT)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable { onOpenRecent(game.path) }
                    .padding(vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    game.title,
                    color = Color.White,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(game.ago, color = Color.Gray, fontSize = 12.sp)
            }
        }
    }
}

@Composable
private fun Toolbar(
    hasGame: Boolean,
    paused: Boolean,
    muted: Boolean,
    recent: List<RecentRom>,
    onOpenRom: () -> Unit,
    onOpenRecent: (String) -> Unit,
    onSlots: () -> Unit,
    onPause: () -> Unit,
    onMute: () -> Unit,
    onSettings: () -> Unit,
    onDebugger: () -> Unit,
    onCheats: () -> Unit,
    onScreenshot: () -> Unit,
    onRestart: () -> Unit,
    onEditPad: () -> Unit,
    onLink: () -> Unit,
    onCheckUpdate: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    var romMenuOpen by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Box {
            // 최근 게임이 없으면 메뉴 없이 바로 파일을 고른다.
            TextButton(onClick = { if (recent.isEmpty()) onOpenRom() else romMenuOpen = true }) { Text("ROM") }
            DropdownMenu(expanded = romMenuOpen, onDismissRequest = { romMenuOpen = false }) {
                DropdownMenuItem(
                    text = { Text("파일에서 열기…") },
                    onClick = { romMenuOpen = false; onOpenRom() },
                )
                for (game in recent) {
                    DropdownMenuItem(
                        text = { Text(game.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        trailingIcon = { Text(game.ago, fontSize = 12.sp, color = Color.Gray) },
                        onClick = { romMenuOpen = false; onOpenRecent(game.path) },
                    )
                }
            }
        }
        TextButton(onClick = onSlots, enabled = hasGame) { Text("스테이트") }
        TextButton(onClick = onPause, enabled = hasGame) { Text(if (paused) "계속" else "일시정지") }
        TextButton(onClick = onMute) { Text(if (muted) "소리 켜기" else "음소거") }
        Spacer(Modifier.weight(1f))
        Box {
            TextButton(onClick = { menuOpen = true }) { Text("⋮", fontSize = 18.sp) }
            DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                DropdownMenuItem(text = { Text("설정") }, onClick = { menuOpen = false; onSettings() })
                DropdownMenuItem(
                    text = { Text("스크린샷") },
                    enabled = hasGame,
                    onClick = { menuOpen = false; onScreenshot() },
                )
                DropdownMenuItem(
                    text = { Text("처음부터 다시 시작") },
                    enabled = hasGame,
                    onClick = { menuOpen = false; onRestart() },
                )
                DropdownMenuItem(
                    text = { Text("패드 위치 편집") },
                    onClick = { menuOpen = false; onEditPad() },
                )
                DropdownMenuItem(
                    text = { Text("링크 케이블") },
                    enabled = hasGame,
                    onClick = { menuOpen = false; onLink() },
                )
                DropdownMenuItem(
                    text = { Text("치트") },
                    enabled = hasGame,
                    onClick = { menuOpen = false; onCheats() },
                )
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

/**
 * A/B와 배속·되감기 버튼. 배속과 되감기는 누르는 동안만 동작한다 (데스크톱 키와 같다).
 * 되감기를 끄면 되감기 버튼은 숨긴다.
 */
@Composable
private fun ActionButtons(viewModel: GameViewModel, rewind: Boolean) {
    val scale = LocalPadScale.current
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            PadButton("B", GbButton.B, viewModel::setButton, modifier = Modifier.padding(top = 32.dp * scale))
            Spacer(Modifier.width(16.dp * scale))
            PadButton("A", GbButton.A, viewModel::setButton, modifier = Modifier.padding(bottom = 32.dp * scale))
        }
        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (rewind) {
                HoldButton("◀◀ 되감기", onHold = viewModel::setRewindTouch, shape = PillShape, width = 80.dp, height = 28.dp)
            }
            HoldButton("▶▶ 배속", onHold = viewModel::setFastTouch, shape = PillShape, width = 80.dp, height = 28.dp)
        }
    }
}

/**
 * 방향키. 손가락을 떼지 않고 미끄러뜨려 방향을 바꿀 수 있고, 대각선도 누를 수 있다.
 * 가운데에서 가까우면(지름의 약 1/5) 아무 방향도 누르지 않는다.
 */
@Composable
private fun DPad(onButton: (GbButton, Boolean) -> Unit) {
    val key = 52.dp * LocalPadScale.current
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
    val scale = LocalPadScale.current
    HoldButton(label, { onButton(button, it) }, shape, width * scale, height * scale, modifier, color)
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

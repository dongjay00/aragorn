package io.github.dongjay00.aragorn

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.core.Emulator
import io.github.dongjay00.aragorn.core.GbButton
import io.github.dongjay00.aragorn.core.screenSize
import java.nio.ByteBuffer

private val Body = Color(0xFF1E1E24)
private val ButtonColor = Color(0xFF3A3A44)
private val ActionColor = Color(0xFF8B2252)

@Composable
fun GameScreen(viewModel: GameViewModel, onOpenRom: () -> Unit) {
    val emulator by viewModel.emulator.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    Column(
        Modifier
            .fillMaxSize()
            .background(Body)
            .safeDrawingPadding()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            TextButton(onClick = onOpenRom) { Text("ROM 열기") }
            Row {
                TextButton(onClick = { viewModel.saveState(0) }, enabled = emulator != null) {
                    Text("저장")
                }
                TextButton(onClick = { viewModel.loadState(0) }, enabled = emulator != null) {
                    Text("불러오기")
                }
            }
        }
        val current = emulator
        if (current != null) {
            LcdScreen(current, onFrame = viewModel::collectErrors)
        } else {
            Box(
                Modifier
                    .fillMaxWidth()
                    .aspectRatio(160f / 144f)
                    .background(Color.Black),
                contentAlignment = Alignment.Center,
            ) {
                Text("ROM을 열어 주세요", color = Color.Gray)
            }
        }
        Text(
            notice ?: "",
            color = Color.LightGray,
            fontSize = 12.sp,
            modifier = Modifier.padding(vertical = 8.dp),
        )
        Spacer(Modifier.weight(1f))
        GamePad(onButton = viewModel::setButton)
    }
}

/**
 * 게임 화면. 디스플레이 갱신(vsync)마다 프레임 번호를 보고, 바뀌었을 때만 픽셀을 가져와 그린다.
 * 확대할 때 픽셀이 뭉개지지 않게 가장 가까운 이웃으로 늘린다.
 */
@Composable
private fun LcdScreen(emulator: Emulator, onFrame: () -> Unit) {
    val size = remember { screenSize() }
    val bitmap = remember {
        Bitmap.createBitmap(size.width.toInt(), size.height.toInt(), Bitmap.Config.ARGB_8888)
    }
    val image = remember(bitmap) { bitmap.asImageBitmap() }
    // 다시 그리기를 일으키는 용도. 값 자체는 쓰지 않는다.
    val redraw = remember { mutableIntStateOf(0) }
    LaunchedEffect(emulator) {
        var shown = ULong.MAX_VALUE
        while (true) {
            withFrameNanos { }
            val number = emulator.frameNumber()
            if (number != shown) {
                shown = number
                bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(emulator.frame()))
                redraw.intValue++
                onFrame()
            }
        }
    }
    Canvas(
        Modifier
            .fillMaxWidth()
            .aspectRatio(size.width.toFloat() / size.height.toFloat())
            .background(Color.Black),
    ) {
        redraw.intValue
        drawImage(
            image,
            srcOffset = IntOffset.Zero,
            srcSize = IntSize(image.width, image.height),
            dstSize = IntSize(this.size.width.toInt(), this.size.height.toInt()),
            filterQuality = FilterQuality.None,
        )
    }
}

/** 화면 아래 가상 패드: 왼쪽 방향키, 오른쪽 A/B, 아래 SELECT/START. */
@Composable
private fun GamePad(onButton: (GbButton, Boolean) -> Unit) {
    Column(Modifier.fillMaxWidth().padding(bottom = 24.dp)) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            DPad(onButton)
            Row(verticalAlignment = Alignment.CenterVertically) {
                PadButton("B", GbButton.B, onButton, Modifier.padding(top = 32.dp))
                Spacer(Modifier.width(16.dp))
                PadButton("A", GbButton.A, onButton, Modifier.padding(bottom = 32.dp))
            }
        }
        Spacer(Modifier.height(24.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
            PadButton("SELECT", GbButton.SELECT, onButton, shape = PillShape, width = 72.dp, height = 28.dp)
            Spacer(Modifier.width(16.dp))
            PadButton("START", GbButton.START, onButton, shape = PillShape, width = 72.dp, height = 28.dp)
        }
    }
}

private val PillShape = RoundedCornerShape(50)
private val KeyShape = RoundedCornerShape(8.dp)

@Composable
private fun DPad(onButton: (GbButton, Boolean) -> Unit) {
    val key = 52.dp
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        PadButton("▲", GbButton.UP, onButton, shape = KeyShape, width = key)
        Row {
            PadButton("◀", GbButton.LEFT, onButton, shape = KeyShape, width = key)
            Spacer(Modifier.size(key))
            PadButton("▶", GbButton.RIGHT, onButton, shape = KeyShape, width = key)
        }
        PadButton("▼", GbButton.DOWN, onButton, shape = KeyShape, width = key)
    }
}

/**
 * 누르는 동안 눌린 버튼. 버튼마다 따로 터치를 받으므로 방향키와 A를 동시에 누를 수 있다.
 */
@Composable
private fun PadButton(
    label: String,
    button: GbButton,
    onButton: (GbButton, Boolean) -> Unit,
    modifier: Modifier = Modifier,
    shape: Shape = CircleShape,
    width: Dp = 64.dp,
    height: Dp = width,
) {
    Box(
        modifier
            .size(width, height)
            .clip(shape)
            .background(if (button == GbButton.A || button == GbButton.B) ActionColor else ButtonColor)
            .pointerInput(button) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    onButton(button, true)
                    waitForUpOrCancellation()
                    onButton(button, false)
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Color.White, style = MaterialTheme.typography.labelMedium)
    }
}

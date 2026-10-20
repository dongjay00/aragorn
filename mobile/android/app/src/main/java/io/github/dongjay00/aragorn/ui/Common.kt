package io.github.dongjay00.aragorn.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.sp
import io.github.dongjay00.aragorn.core.Emulator
import io.github.dongjay00.aragorn.core.Image
import io.github.dongjay00.aragorn.core.screenSize
import java.nio.ByteBuffer

val Mono = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 12.sp)

/** Rust가 준 RGBA 이미지를 Compose 이미지로 바꾼다. */
fun Image.toImageBitmap(): ImageBitmap {
    val bitmap = Bitmap.createBitmap(width.toInt(), height.toInt(), Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
    return bitmap.asImageBitmap()
}

/** 0xRRGGBBAA → Compose 색. */
fun rgbaColor(rgba: UInt): Color {
    val v = rgba.toLong()
    return Color(
        red = ((v shr 24) and 0xFF).toInt(),
        green = ((v shr 16) and 0xFF).toInt(),
        blue = ((v shr 8) and 0xFF).toInt(),
        alpha = (v and 0xFF).toInt(),
    )
}

/**
 * 게임 화면. 디스플레이 갱신(vsync)마다 프레임 번호를 보고, 바뀌었을 때만 픽셀을 가져와 그린다.
 * 슈퍼 게임보이 테두리를 그리면 화면이 256×224로 바뀌므로 크기가 바뀔 때 비트맵을 다시 만든다.
 * 확대할 때 픽셀이 뭉개지지 않게 가장 가까운 이웃으로 늘린다. `onFrame`은 화면 갱신마다 부른다.
 * LCD 격자는 테두리가 없을 때만 그린다.
 */
@Composable
fun LcdScreen(
    emulator: Emulator,
    modifier: Modifier = Modifier,
    grid: Boolean = false,
    onFrame: () -> Unit = {},
) {
    val initial = remember { screenSize() }
    var bitmap by remember {
        mutableStateOf(Bitmap.createBitmap(initial.width.toInt(), initial.height.toInt(), Bitmap.Config.ARGB_8888))
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
                val frame = emulator.frame()
                if (frame.width.toInt() != bitmap.width || frame.height.toInt() != bitmap.height) {
                    bitmap = Bitmap.createBitmap(frame.width.toInt(), frame.height.toInt(), Bitmap.Config.ARGB_8888)
                }
                bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(frame.rgba))
                redraw.intValue++
            }
            onFrame()
        }
    }
    val bordered = image.width != initial.width.toInt()
    Canvas(
        modifier
            .aspectRatio(image.width.toFloat() / image.height.toFloat())
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
        if (grid && !bordered) drawLcdGrid(image.width, image.height)
    }
}

/** LCD 격자를 그리는 최소 배율. 이보다 작으면 격자가 화면을 가린다. */
private const val GRID_MIN_SCALE = 3f
private val GridColor = Color.Black.copy(alpha = 0.25f)

/** 픽셀 사이에 어두운 선을 그려 LCD처럼 보이게 한다. */
private fun DrawScope.drawLcdGrid(columns: Int, rows: Int) {
    val cell = size.width / columns
    if (cell < GRID_MIN_SCALE) return
    val stroke = maxOf(1f, cell / 6f)
    for (x in 1 until columns) {
        drawLine(GridColor, Offset(x * cell, 0f), Offset(x * cell, size.height), stroke)
    }
    for (y in 1 until rows) {
        drawLine(GridColor, Offset(0f, y * cell), Offset(size.width, y * cell), stroke)
    }
}

/** 픽셀 이미지를 정해진 배율로 그린다 (VRAM, 스프라이트, 썸네일). */
@Composable
fun PixelImage(image: ImageBitmap, modifier: Modifier = Modifier) {
    Canvas(modifier.aspectRatio(image.width.toFloat() / image.height.toFloat())) {
        drawImage(
            image,
            srcOffset = IntOffset.Zero,
            srcSize = IntSize(image.width, image.height),
            dstSize = IntSize(size.width.toInt(), size.height.toInt()),
            filterQuality = FilterQuality.None,
        )
    }
}

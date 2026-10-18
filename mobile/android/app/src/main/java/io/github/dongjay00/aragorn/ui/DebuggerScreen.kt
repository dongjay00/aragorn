package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel
import io.github.dongjay00.aragorn.core.CpuView
import io.github.dongjay00.aragorn.core.Emulator
import io.github.dongjay00.aragorn.core.IoGroup
import io.github.dongjay00.aragorn.core.MapView
import io.github.dongjay00.aragorn.core.PaletteGroup
import io.github.dongjay00.aragorn.core.SpriteView
import io.github.dongjay00.aragorn.core.memoryRowCount
import io.github.dongjay00.aragorn.core.parseAddress
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** 디버거가 화면을 다시 읽는 간격. 게임이 도는 동안에도 값이 따라 바뀐다. */
private const val REFRESH_MS = 200L

private enum class DebugTab(val title: String) { CPU("CPU"), MEMORY("메모리"), VRAM("VRAM"), IO("I/O") }

/** 데스크톱 디버거 창(CPU·메모리·VRAM·I/O)을 탭 하나씩으로 보인다. 위에는 게임 화면과 실행 제어가 있다. */
@Composable
fun DebuggerScreen(viewModel: GameViewModel, onBack: () -> Unit) {
    val emulator by viewModel.emulator.collectAsStateWithLifecycle()
    val paused by viewModel.paused.collectAsStateWithLifecycle()
    val speedText by viewModel.speedText.collectAsStateWithLifecycle()
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(DebugTab.CPU) }
    val current = emulator
    if (current == null) {
        // 게임이 없으면 볼 것이 없다.
        LaunchedEffect(Unit) { onBack() }
        return
    }
    // 값을 다시 읽을 때마다 늘어난다.
    val tick by produceState(0) {
        while (true) {
            delay(REFRESH_MS)
            value++
        }
    }

    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onBack) { Text("← 돌아가기") }
                Text("디버거", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                Text(speedText, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(end = 16.dp))
            }
            Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                LcdScreen(current, Modifier.height(120.dp), onFrame = viewModel::poll)
                Column(Modifier.padding(start = 12.dp)) {
                    OutlinedButton(onClick = viewModel::togglePause) { Text(if (paused) "계속" else "일시정지") }
                    Row {
                        TextButton(onClick = current::stepInstruction) { Text("명령 1개") }
                        TextButton(onClick = current::stepFrame) { Text("프레임 1개") }
                    }
                }
            }
            notice?.let { Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp)) }
            PrimaryTabRow(selectedTabIndex = tab.ordinal) {
                for (t in DebugTab.entries) {
                    Tab(selected = t == tab, onClick = { tab = t }, text = { Text(t.title) })
                }
            }
            Box(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                when (tab) {
                    DebugTab.CPU -> CpuTab(viewModel, current, tick)
                    DebugTab.MEMORY -> MemoryTab(current, tick)
                    DebugTab.VRAM -> VramTab(current, tick)
                    DebugTab.IO -> IoTab(viewModel, current, tick)
                }
            }
        }
    }
}

/** 에뮬레이터 스레드에 묻는 일을 UI 스레드 밖에서 하고, `tick`마다 다시 묻는다. */
@Composable
private fun <T> rememberSnapshot(emulator: Emulator, tick: Int, vararg keys: Any?, read: (Emulator) -> T): T? {
    val state = remember(emulator, *keys) { mutableStateOf<T?>(null) }
    LaunchedEffect(emulator, tick, *keys) {
        state.value = withContext(Dispatchers.Default) { runCatching { read(emulator) }.getOrNull() }
    }
    return state.value
}

// ---- CPU ----

@Composable
private fun CpuTab(viewModel: GameViewModel, emulator: Emulator, tick: Int) {
    val cpu: CpuView? = rememberSnapshot(emulator, tick) { it.cpu() }
    val breakpoints by viewModel.breakpoints.collectAsStateWithLifecycle()
    var input by rememberSaveable { mutableStateOf("") }
    Column(Modifier.verticalScroll(rememberScrollState())) {
        cpu ?: return@Column
        for (notice in cpu.notices) {
            Text(notice, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        for (line in cpu.registers) Text(line, style = Mono)
        Spacer(Modifier.height(8.dp))
        for (row in cpu.rows) {
            val on = row.pc in breakpoints
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable { viewModel.toggleBreakpoint(row.pc) },
            ) {
                Text(if (on) "● " else "○ ", style = Mono, color = MaterialTheme.colorScheme.error)
                Text(
                    row.text,
                    style = Mono,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (row.ahead) 1f else 0.5f),
                )
            }
        }
        Text(
            "줄을 누르면 브레이크포인트를 켜고 끕니다.",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(vertical = 8.dp),
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            val add = {
                parseAddress(input)?.let {
                    if (it !in breakpoints) viewModel.toggleBreakpoint(it)
                    input = ""
                }
            }
            OutlinedTextField(
                value = input,
                onValueChange = { input = it },
                label = { Text("브레이크포인트 주소") },
                placeholder = { Text("0150") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Characters,
                    imeAction = ImeAction.Done,
                ),
                keyboardActions = KeyboardActions(onDone = { add() }),
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { add() }) { Text("추가") }
        }
        for (pc in breakpoints) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("$%04X".format(pc.toInt()), style = Mono, modifier = Modifier.weight(1f))
                TextButton(onClick = { viewModel.toggleBreakpoint(pc) }) { Text("✕") }
            }
        }
    }
}

// ---- 메모리 ----

@Composable
private fun MemoryTab(emulator: Emulator, tick: Int) {
    val total = remember { memoryRowCount().toInt() }
    val list = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val rows = remember(emulator) { mutableStateMapOf<Int, String>() }
    var address by rememberSaveable { mutableStateOf("") }
    // 보이는 줄만 다시 읽는다.
    LaunchedEffect(emulator) {
        snapshotFlow { list.firstVisibleItemIndex to list.layoutInfo.visibleItemsInfo.size }
            .combine(snapshotFlow { tick }) { range, _ -> range }
            .collect { (first, visible) ->
                val count = (visible + 4).coerceAtLeast(32)
                val lines = withContext(Dispatchers.Default) {
                    runCatching { emulator.memoryRows(first.toUInt(), count.toUInt()) }.getOrNull()
                } ?: return@collect
                lines.forEachIndexed { i, line -> rows[first + i] = line }
            }
    }
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            val go = {
                parseAddress(address)?.let { addr -> scope.launch { list.scrollToItem(addr.toInt() / 16) } }
            }
            OutlinedTextField(
                value = address,
                onValueChange = { address = it },
                label = { Text("주소") },
                placeholder = { Text("C000") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Characters,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { go() }),
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = { go() }) { Text("이동") }
        }
        LazyColumn(state = list, modifier = Modifier.horizontalScroll(rememberScrollState())) {
            items(total) { row -> Text(rows[row] ?: "%04X".format(row * 16), style = Mono) }
        }
    }
}

// ---- VRAM ----

private enum class VramPage(val title: String) { TILES("타일"), MAP("배경 맵"), OAM("OAM"), PALETTES("팔레트") }

@Composable
private fun VramTab(emulator: Emulator, tick: Int) {
    var page by rememberSaveable { mutableStateOf(VramPage.TILES) }
    Column {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (p in VramPage.entries) {
                FilterChip(selected = p == page, onClick = { page = p }, label = { Text(p.title) })
            }
        }
        when (page) {
            VramPage.TILES -> TilesPage(emulator, tick)
            VramPage.MAP -> MapPage(emulator, tick)
            VramPage.OAM -> OamPage(emulator, tick)
            VramPage.PALETTES -> PalettesPage(emulator, tick)
        }
    }
}

@Composable
private fun TilesPage(emulator: Emulator, tick: Int) {
    val cgb = remember(emulator) { emulator.isCgb() }
    var bank by rememberSaveable { mutableStateOf(0) }
    val image: ImageBitmap? = rememberSnapshot(emulator, tick, bank) { it.tiles(bank.toUByte()).toImageBitmap() }
    Column(Modifier.verticalScroll(rememberScrollState())) {
        if (cgb) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (b in 0..1) FilterChip(selected = bank == b, onClick = { bank = b }, label = { Text("뱅크 $b") })
            }
        }
        image?.let { PixelImage(it, Modifier.fillMaxWidth()) }
        Text("타일 하나가 16바이트, \$8000부터 384개", style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
private fun MapPage(emulator: Emulator, tick: Int) {
    var window by rememberSaveable { mutableStateOf(false) }
    val map: Pair<MapView, ImageBitmap>? = rememberSnapshot(emulator, tick, window) {
        val view = it.tileMap(window)
        view to view.image.toImageBitmap()
    }
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            FilterChip(selected = !window, onClick = { window = false }, label = { Text("배경") })
            FilterChip(selected = window, onClick = { window = true }, label = { Text("윈도우") })
        }
        map?.let { (view, image) ->
            Text(view.label, style = MaterialTheme.typography.bodySmall)
            PixelImage(image, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun OamPage(emulator: Emulator, tick: Int) {
    val sprites: List<Pair<SpriteView, ImageBitmap>>? = rememberSnapshot(emulator, tick) {
        it.sprites().map { sprite -> sprite to sprite.image.toImageBitmap() }
    }
    LazyColumn {
        item {
            Row {
                for ((title, weight) in listOf("#" to 0.6f, "" to 0.8f, "X" to 0.8f, "Y" to 0.8f, "타일" to 1f, "속성" to 3f)) {
                    Text(title, Modifier.weight(weight), style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        items(sprites.orEmpty(), key = { it.first.index.toInt() }) { (sprite, image) ->
            val alpha = if (sprite.onScreen) 1f else 0.45f
            val color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha)
            Row(Modifier.padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(sprite.index.toString(), Modifier.weight(0.6f), style = Mono, color = color)
                Box(Modifier.weight(0.8f)) { PixelImage(image, Modifier.height(if (image.height > 8) 48.dp else 24.dp)) }
                Text("%3d".format(sprite.x.toInt()), Modifier.weight(0.8f), style = Mono, color = color)
                Text("%3d".format(sprite.y.toInt()), Modifier.weight(0.8f), style = Mono, color = color)
                Text("$%02X".format(sprite.tile.toInt()), Modifier.weight(1f), style = Mono, color = color)
                Text(sprite.attributes, Modifier.weight(3f), style = Mono, color = color)
            }
        }
    }
}

@Composable
private fun PalettesPage(emulator: Emulator, tick: Int) {
    val groups: List<PaletteGroup>? = rememberSnapshot(emulator, tick) { it.palettes() }
    var picked by remember { mutableStateOf("색을 누르면 값이 보입니다") }
    Column(Modifier.verticalScroll(rememberScrollState())) {
        Text(picked, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(vertical = 4.dp))
        for (group in groups.orEmpty()) {
            Text(group.title, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 8.dp))
            for (row in group.rows) {
                Row(Modifier.padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(row.label, Modifier.width(96.dp), style = Mono)
                    for (swatch in row.colors) {
                        Box(
                            Modifier
                                .padding(end = 6.dp)
                                .size(28.dp)
                                .background(rgbaColor(swatch.rgba))
                                .clickable { picked = "${group.title} ${row.label} · ${swatch.label}" },
                        )
                    }
                }
            }
        }
    }
}

// ---- I/O ----

@Composable
private fun IoTab(viewModel: GameViewModel, emulator: Emulator, tick: Int) {
    val groups: List<IoGroup>? = rememberSnapshot(emulator, tick) { it.ioRegisters() }
    val muted by viewModel.channelMuted.collectAsStateWithLifecycle()
    LazyColumn {
        item {
            Text("음소거", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 8.dp))
            Row(Modifier.horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
                for (channel in 0 until 4) {
                    Checkbox(checked = muted[channel], onCheckedChange = { viewModel.setChannelMuted(channel, it) })
                    Text("채널 ${channel + 1}")
                    Spacer(Modifier.width(8.dp))
                }
            }
        }
        for (group in groups.orEmpty()) {
            item(key = group.name) {
                Text(group.name, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 12.dp))
            }
            items(group.rows, key = { "${group.name}-${it.address}" }) { row ->
                Row(Modifier.padding(vertical = 1.dp)) {
                    Text(
                        "%-5s $%04X %02X".format(row.name, row.address.toInt(), row.value.toInt()),
                        style = Mono,
                    )
                    Text(
                        row.description,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(start = 8.dp),
                    )
                }
            }
        }
    }
}

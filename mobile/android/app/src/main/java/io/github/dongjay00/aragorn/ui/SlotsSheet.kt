package io.github.dongjay00.aragorn.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dongjay00.aragorn.GameViewModel

/** 슬롯 수 (데스크톱과 같다). 키보드 F1–F10이 슬롯 1–10이다. */
private const val SLOT_COUNT = 10

/** 스테이트 슬롯 10개: 썸네일, 저장 시각, 저장·불러오기 (데스크톱 "스테이트" 메뉴와 같다). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SlotsSheet(viewModel: GameViewModel, onDismiss: () -> Unit) {
    val slots by viewModel.slots.collectAsStateWithLifecycle()
    val bySlot = remember(slots) { slots.associateBy { it.slot.toInt() } }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Text(
            "스테이트 · 키보드 F1–F10 저장, Shift+F1–F10 불러오기",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(horizontal = 16.dp),
        )
        LazyVerticalGrid(
            columns = GridCells.Adaptive(minSize = 150.dp),
            modifier = Modifier.padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items((0 until SLOT_COUNT).toList()) { slot ->
                val summary = bySlot[slot]
                Card {
                    Column(Modifier.padding(8.dp)) {
                        val thumbnail = summary?.thumbnail
                        val image = remember(thumbnail) { thumbnail?.toImageBitmap() }
                        if (image != null) {
                            PixelImage(image, Modifier.fillMaxWidth())
                        } else {
                            Box(
                                Modifier.fillMaxWidth().aspectRatio(80f / 72f).background(Color.DarkGray),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text("비어 있음", color = Color.Gray)
                            }
                        }
                        Text(
                            "슬롯 ${slot + 1}" + (summary?.let { " · ${it.ago}" } ?: ""),
                            style = MaterialTheme.typography.labelMedium,
                            modifier = Modifier.padding(top = 4.dp),
                        )
                        Row {
                            TextButton(onClick = { viewModel.saveState(slot); onDismiss() }) {
                                Text("저장")
                            }
                            TextButton(
                                onClick = { viewModel.loadState(slot); onDismiss() },
                                enabled = summary != null,
                            ) {
                                Text("불러오기")
                            }
                        }
                    }
                }
            }
        }
    }
}

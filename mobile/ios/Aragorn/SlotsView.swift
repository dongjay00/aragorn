import SwiftUI

/// 스테이트 슬롯 10개: 썸네일, 저장 시각, 저장·불러오기 (데스크톱 "스테이트" 메뉴와 같다).
struct SlotsView: View {
    @Environment(GameModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let bySlot = Dictionary(uniqueKeysWithValues: model.slots.map { (Int($0.slot), $0) })
        let count = Int(model.emulator?.slotCount() ?? 10)
        NavigationStack {
            ScrollView {
                Text("키보드 F1–F10 저장, Shift+F1–F10 불러오기")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                    ForEach(0..<count, id: \.self) { slot in
                        card(slot: slot, summary: bySlot[slot])
                    }
                }
                .padding()
            }
            .navigationTitle("스테이트")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("닫기") { dismiss() } }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func card(slot: Int, summary: SlotSummary?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let image = summary?.thumbnail?.cgImage {
                PixelImage(image: image)
            } else {
                Rectangle()
                    .fill(Color(white: 0.25))
                    .aspectRatio(80 / 72, contentMode: .fit)
                    .overlay(Text("비어 있음").foregroundStyle(.gray))
            }
            Text("슬롯 \(slot + 1)" + (summary.map { " · \($0.ago)" } ?? ""))
                .font(.caption)
            HStack {
                Button("저장") {
                    model.saveState(slot)
                    dismiss()
                }
                Spacer()
                Button("불러오기") {
                    model.loadState(slot)
                    dismiss()
                }
                .disabled(summary == nil)
            }
            .buttonStyle(.borderless)
        }
        .padding(8)
        .background(Color(white: 0.15), in: RoundedRectangle(cornerRadius: 12))
    }
}

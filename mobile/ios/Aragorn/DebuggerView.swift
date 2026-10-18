import SwiftUI

/// 디버거가 화면을 다시 읽는 간격. 게임이 도는 동안에도 값이 따라 바뀐다.
private let refreshInterval: Duration = .milliseconds(200)

private enum DebugTab: String, CaseIterable, Identifiable {
    case cpu = "CPU", memory = "메모리", vram = "VRAM", io = "I/O"
    var id: Self { self }
}

/// 데스크톱 디버거 창(CPU·메모리·VRAM·I/O)을 탭 하나씩으로 보인다. 위에는 게임 화면과 실행 제어가 있다.
struct DebuggerView: View {
    @Environment(GameModel.self) private var model
    @State private var tab = DebugTab.cpu

    var body: some View {
        NavigationStack {
            if let emulator = model.emulator {
                VStack(spacing: 8) {
                    HStack(alignment: .center, spacing: 12) {
                        LcdView(emulator: emulator, onFrame: model.poll)
                            .aspectRatio(160 / 144, contentMode: .fit)
                            .frame(height: 120)
                        VStack(alignment: .leading) {
                            Button(model.paused ? "계속" : "일시정지", action: model.togglePause)
                                .buttonStyle(.bordered)
                            HStack {
                                Button("명령 1개") { emulator.stepInstruction() }
                                Button("프레임 1개") { emulator.stepFrame() }
                            }
                            .buttonStyle(.borderless)
                        }
                        Spacer()
                    }
                    if let notice = model.notice {
                        Text(notice).font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                            .accessibilityIdentifier("status.notice")
                    }
                    Picker("탭", selection: $tab) {
                        ForEach(DebugTab.allCases) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    switch tab {
                    case .cpu: CpuTab(emulator: emulator)
                    case .memory: MemoryTab(emulator: emulator)
                    case .vram: VramTab(emulator: emulator)
                    case .io: IoTab(emulator: emulator)
                    }
                }
                .padding(.horizontal)
                .navigationTitle("디버거")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("돌아가기") { model.page = .game } }
                    ToolbarItem(placement: .primaryAction) { Text(model.speedText).font(.caption) }
                }
            } else {
                // 게임이 없으면 볼 것이 없다.
                Color.clear.onAppear { model.page = .game }
            }
        }
    }
}

/// 에뮬레이터 스레드에 묻는 일을 메인 스레드 밖에서 하고, `refreshInterval`마다 다시 묻는다.
private struct Snapshot<Value: Sendable, Content: View>: View {
    let emulator: Emulator
    let key: AnyHashable
    let read: @Sendable (Emulator) throws -> Value
    @ViewBuilder let content: (Value) -> Content
    @State private var value: Value?

    var body: some View {
        Group {
            if let value { content(value) } else { ProgressView().frame(maxHeight: .infinity) }
        }
        .task(id: key) {
            while !Task.isCancelled {
                let emulator = emulator
                let read = read
                value = await Task.detached { try? read(emulator) }.value ?? value
                try? await Task.sleep(for: refreshInterval)
            }
        }
    }
}

// MARK: CPU

private struct CpuTab: View {
    @Environment(GameModel.self) private var model
    let emulator: Emulator
    @State private var input = ""

    var body: some View {
        Snapshot(emulator: emulator, key: 0, read: { try $0.cpu() }) { cpu in
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(cpu.notices, id: \.self) { Text($0).font(.caption).foregroundStyle(.red) }
                    ForEach(cpu.registers, id: \.self) { Text($0).font(mono) }
                    Spacer().frame(height: 8)
                    ForEach(cpu.rows, id: \.pc) { row in
                        HStack(spacing: 4) {
                            Text(model.breakpoints.contains(row.pc) ? "●" : "○").foregroundStyle(.red)
                            Text(row.text).opacity(row.ahead ? 1 : 0.5)
                        }
                        .font(mono)
                        .contentShape(Rectangle())
                        .onTapGesture { model.toggleBreakpoint(row.pc) }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier(String(format: "disasm.%04X", row.pc))
                    }
                    Text("줄을 누르면 브레이크포인트를 켜고 끕니다.").font(.caption).foregroundStyle(.secondary)
                        .padding(.vertical, 8)
                    HStack {
                        TextField("브레이크포인트 주소 (0150)", text: $input)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(add)
                        Button("추가", action: add)
                    }
                    ForEach(model.breakpoints, id: \.self) { pc in
                        HStack {
                            Text(String(format: "$%04X", pc)).font(mono)
                            Spacer()
                            Button("✕") { model.toggleBreakpoint(pc) }.buttonStyle(.borderless)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private func add() {
        guard let pc = parseAddress(text: input) else { return }
        if !model.breakpoints.contains(pc) { model.toggleBreakpoint(pc) }
        input = ""
    }
}

// MARK: 메모리

private struct MemoryTab: View {
    let emulator: Emulator
    @State private var address = ""
    /// 보이는 줄. 이 범위만 다시 읽는다.
    @State private var visible: Set<Int> = []
    @State private var rows: [Int: String] = [:]
    private let total = Int(memoryRowCount())

    var body: some View {
        ScrollViewReader { proxy in
            VStack {
                HStack {
                    TextField("주소 (C000)", text: $address)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { jump(proxy) }
                    Button("이동") { jump(proxy) }
                }
                ScrollView([.vertical, .horizontal]) {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        ForEach(0..<total, id: \.self) { row in
                            Text(rows[row] ?? String(format: "%04X", row * 16))
                                .font(mono)
                                .id(row)
                                .onAppear { visible.insert(row) }
                                .onDisappear { visible.remove(row) }
                        }
                    }
                }
            }
        }
        .task {
            while !Task.isCancelled {
                if let first = visible.min(), let last = visible.max() {
                    let emulator = emulator
                    let lines = await Task.detached {
                        try? emulator.memoryRows(first: UInt32(first), count: UInt32(last - first + 1))
                    }.value ?? []
                    for (i, line) in lines.enumerated() { rows[first + i] = line }
                }
                try? await Task.sleep(for: refreshInterval)
            }
        }
    }

    private func jump(_ proxy: ScrollViewProxy) {
        guard let addr = parseAddress(text: address) else { return }
        proxy.scrollTo(Int(addr) / 16, anchor: .top)
    }
}

// MARK: VRAM

private enum VramPage: String, CaseIterable, Identifiable {
    case tiles = "타일", map = "배경 맵", oam = "OAM", palettes = "팔레트"
    var id: Self { self }
}

private struct VramTab: View {
    let emulator: Emulator
    @State private var page = VramPage.tiles
    @State private var bank: UInt8 = 0
    @State private var window = false
    @State private var picked = "색을 누르면 값이 보입니다"

    var body: some View {
        VStack {
            Picker("VRAM", selection: $page) {
                ForEach(VramPage.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            switch page {
            case .tiles: tiles
            case .map: map
            case .oam: oam
            case .palettes: palettes
            }
        }
    }

    private var tiles: some View {
        VStack {
            if emulator.isCgb() {
                Picker("뱅크", selection: $bank) {
                    Text("뱅크 0").tag(UInt8(0))
                    Text("뱅크 1").tag(UInt8(1))
                }
                .pickerStyle(.segmented)
            }
            Snapshot(emulator: emulator, key: bank, read: { [bank] in try $0.tiles(bank: bank) }) { image in
                ScrollView {
                    if let cg = image.cgImage { PixelImage(image: cg) }
                    Text("타일 하나가 16바이트, $8000부터 384개").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    private var map: some View {
        VStack {
            Picker("맵", selection: $window) {
                Text("배경").tag(false)
                Text("윈도우").tag(true)
            }
            .pickerStyle(.segmented)
            Snapshot(emulator: emulator, key: window, read: { [window] in try $0.tileMap(window: window) }) { view in
                ScrollView {
                    Text(view.label).font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                    if let cg = view.image.cgImage { PixelImage(image: cg) }
                }
            }
        }
    }

    private var oam: some View {
        Snapshot(emulator: emulator, key: 0, read: { try $0.sprites() }) { sprites in
            ScrollView {
                Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 4) {
                    GridRow {
                        ForEach(["#", "", "X", "Y", "타일", "속성"], id: \.self) { Text($0).font(.caption.bold()) }
                    }
                    ForEach(sprites, id: \.index) { sprite in
                        GridRow {
                            Text("\(sprite.index)")
                            if let cg = sprite.image.cgImage {
                                PixelImage(image: cg).frame(height: cg.height > 8 ? 48 : 24)
                            } else {
                                Text("")
                            }
                            Text(String(format: "%3d", sprite.x))
                            Text(String(format: "%3d", sprite.y))
                            Text(String(format: "$%02X", sprite.tile))
                            Text(sprite.attributes)
                        }
                        .font(mono)
                        .opacity(sprite.onScreen ? 1 : 0.45)
                    }
                }
            }
        }
    }

    private var palettes: some View {
        Snapshot(emulator: emulator, key: 0, read: { try $0.palettes() }) { groups in
            ScrollView {
                VStack(alignment: .leading, spacing: 4) {
                    Text(picked).font(.caption)
                    ForEach(groups, id: \.title) { group in
                        Text(group.title).font(.subheadline.bold()).padding(.top, 8)
                        ForEach(group.rows, id: \.label) { row in
                            HStack {
                                Text(row.label).font(mono).frame(width: 96, alignment: .leading)
                                ForEach(Array(row.colors.enumerated()), id: \.offset) { _, swatch in
                                    Rectangle()
                                        .fill(rgbaColor(swatch.rgba))
                                        .frame(width: 28, height: 28)
                                        .onTapGesture { picked = "\(group.title) \(row.label) · \(swatch.label)" }
                                }
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

// MARK: I/O

private struct IoTab: View {
    @Environment(GameModel.self) private var model
    let emulator: Emulator

    var body: some View {
        Snapshot(emulator: emulator, key: 0, read: { try $0.ioRegisters() }) { groups in
            List {
                Section("음소거") {
                    ForEach(0..<4, id: \.self) { channel in
                        Toggle(
                            "채널 \(channel + 1)",
                            isOn: Binding(
                                get: { model.channelMuted[channel] },
                                set: { model.setChannelMuted(channel, $0) }
                            )
                        )
                    }
                }
                ForEach(groups, id: \.name) { group in
                    Section(group.name) {
                        ForEach(group.rows, id: \.address) { row in
                            VStack(alignment: .leading) {
                                Text(String(format: "%@ $%04X %02X", row.name.padding(toLength: 5, withPad: " ", startingAt: 0), row.address, row.value))
                                    .font(mono)
                                if !row.description.isEmpty {
                                    Text(row.description).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .listStyle(.plain)
        }
    }
}

import SwiftUI

private let bodyColor = Color(red: 0x1E / 255, green: 0x1E / 255, blue: 0x24 / 255)
private let keyColor = Color(red: 0x3A / 255, green: 0x3A / 255, blue: 0x44 / 255)
private let keyPressed = Color(red: 0x5A / 255, green: 0x5A / 255, blue: 0x68 / 255)
private let actionColor = Color(red: 0x8B / 255, green: 0x22 / 255, blue: 0x52 / 255)
/// 패드를 아래에서 띄우는 최대 높이. 설정의 높이(0–1)에 곱한다.
private let maxLift: CGFloat = 160
/// 배율 1에서 방향키와 A/B를 나란히 놓는 데 필요한 폭 (세로 화면).
private let portraitPadWidth: CGFloat = 320
/// 배율 1에서 가로 화면 한쪽 칸에 필요한 폭과 높이.
private let landscapeSideWidth: CGFloat = 170
private let landscapeSideHeight: CGFloat = 230
/// 툴바 ROM 메뉴와 빈 화면에 보일 최근 게임 수.
private let recentShown = 5

extension EnvironmentValues {
    /// 가상 패드 버튼 크기 배율.
    @Entry var padScale: CGFloat = 1
}

/// 게임 화면: 툴바, 게임 화면, 상태 줄, 가상 패드. 가로로 돌리면 방향키와 A/B가 양옆으로 간다.
struct GameView: View {
    @Environment(GameModel.self) private var model
    let onOpenRom: () -> Void
    @State private var slotsOpen = false

    var body: some View {
        GeometryReader { geometry in
            let landscape = geometry.size.width > geometry.size.height
            let pad = model.padLayout
            let lift = maxLift * CGFloat(pad.lift)
            let lcdWidth = (geometry.size.height - 96) * 160 / 144
            // 설정한 크기가 화면에 들어가지 않으면 들어가는 만큼만 키운다.
            let fit = landscape
                ? min((geometry.size.width - 32 - lcdWidth) / 2 / landscapeSideWidth,
                      (geometry.size.height - 48 - lift) / landscapeSideHeight)
                : (geometry.size.width - 32) / portraitPadWidth
            let scale = min(CGFloat(pad.scale), max(fit, 0.5))
            let opacity = Double(pad.opacity)
            VStack(spacing: 0) {
                toolbar
                if landscape {
                    HStack {
                        VStack {
                            Spacer()
                            DPad(onButton: model.setButton)
                            Spacer()
                            PadButton(label: "SELECT", button: .select, shape: .pill)
                        }
                        .padding(.bottom, lift)
                        .opacity(opacity)
                        .frame(maxWidth: .infinity)
                        VStack(spacing: 0) {
                            screen
                            status
                        }
                        .frame(width: lcdWidth)
                        VStack {
                            Spacer()
                            ActionButtons()
                            Spacer()
                            PadButton(label: "START", button: .start, shape: .pill)
                        }
                        .padding(.bottom, lift)
                        .opacity(opacity)
                        .frame(maxWidth: .infinity)
                    }
                    .padding(.bottom, 8)
                } else {
                    screen
                    status
                    Spacer()
                    HStack {
                        DPad(onButton: model.setButton)
                        Spacer()
                        ActionButtons()
                    }
                    .opacity(opacity)
                    .padding(.bottom, 24)
                    HStack(spacing: 16) {
                        PadButton(label: "SELECT", button: .select, shape: .pill)
                        PadButton(label: "START", button: .start, shape: .pill)
                    }
                    .opacity(opacity)
                    .padding(.bottom, 24 + lift)
                }
            }
            .padding(.horizontal, 16)
            .environment(\.padScale, scale)
        }
        .background(bodyColor.ignoresSafeArea())
        .preferredColorScheme(.dark)
        .sheet(isPresented: $slotsOpen) { SlotsView() }
    }

    private var toolbar: some View {
        HStack(spacing: 4) {
            if model.recent.isEmpty {
                Button("ROM", action: onOpenRom)
            } else {
                Menu("ROM") {
                    Button("파일에서 열기…", action: onOpenRom)
                    Section("최근 게임") {
                        ForEach(model.recent.prefix(recentShown), id: \.path) { game in
                            Button("\(game.title) · \(game.ago)") { model.openRecent(game.path) }
                        }
                    }
                }
            }
            Button("스테이트") {
                model.refreshSlots()
                slotsOpen = true
            }
            .disabled(model.emulator == nil)
            Button(model.paused ? "계속" : "일시정지", action: model.togglePause)
                .disabled(model.emulator == nil)
            Button(model.muted ? "소리 켜기" : "음소거") { model.setMuted(!model.muted) }
            Spacer()
            Menu {
                Button("설정") { model.page = .settings }
                Button("스크린샷", action: model.saveScreenshot)
                    .disabled(model.emulator == nil)
                Button("처음부터 다시 시작", action: model.restart)
                    .disabled(model.emulator == nil)
                Button("치트") { model.page = .cheats }
                    .disabled(model.emulator == nil)
                Button("디버거") { model.page = .debugger }
                    .disabled(model.emulator == nil)
                Button("업데이트 확인") { model.checkForUpdate(manual: true) }
            } label: {
                SwiftUI.Image(systemName: "ellipsis.circle").accessibilityLabel("메뉴")
            }
        }
        .buttonStyle(.borderless)
        .padding(.vertical, 8)
    }

    @ViewBuilder private var screen: some View {
        if let emulator = model.emulator {
            LcdView(emulator: emulator, onFrame: model.poll)
                .aspectRatio(160 / 144, contentMode: .fit)
                .overlay { if model.video.lcdGrid { LcdGrid() } }
        } else {
            Rectangle()
                .fill(.black)
                .aspectRatio(160 / 144, contentMode: .fit)
                .overlay { recentList }
        }
    }

    /// 게임이 없을 때 화면 자리. 최근 게임을 누르면 바로 연다.
    @ViewBuilder private var recentList: some View {
        if model.recent.isEmpty {
            Text("ROM을 열어 주세요").foregroundStyle(.gray)
        } else {
            VStack(spacing: 0) {
                Text("최근 게임").font(.caption).foregroundStyle(.gray).padding(.bottom, 4)
                ForEach(model.recent.prefix(recentShown), id: \.path) { game in
                    Button { model.openRecent(game.path) } label: {
                        HStack {
                            Text(game.title).foregroundStyle(.white).lineLimit(1)
                            Spacer()
                            Text(game.ago).font(.caption).foregroundStyle(.gray)
                        }
                        .padding(.vertical, 6)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("recent.\(game.title)")
                }
            }
            .padding(12)
        }
    }

    private var status: some View {
        HStack {
            Text(model.notice ?? "").lineLimit(1).accessibilityIdentifier("status.notice")
            Spacer()
            if model.emulator != nil { Text(model.speedText).accessibilityIdentifier("status.speed") }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.vertical, 6)
    }
}

/// 픽셀 사이에 어두운 선을 그려 LCD처럼 보이게 한다. 칸이 3포인트보다 작으면 그리지 않는다.
private struct LcdGrid: View {
    var body: some View {
        Canvas { context, size in
            let cell = size.width / 160
            guard cell >= 3 else { return }
            var path = Path()
            for x in 1..<160 {
                path.move(to: CGPoint(x: CGFloat(x) * cell, y: 0))
                path.addLine(to: CGPoint(x: CGFloat(x) * cell, y: size.height))
            }
            for y in 1..<144 {
                path.move(to: CGPoint(x: 0, y: CGFloat(y) * cell))
                path.addLine(to: CGPoint(x: size.width, y: CGFloat(y) * cell))
            }
            context.stroke(path, with: .color(.black.opacity(0.25)), lineWidth: max(0.5, cell / 6))
        }
        .allowsHitTesting(false)
    }
}

/// A/B와 배속·되감기 버튼. 배속과 되감기는 누르는 동안만 동작한다 (데스크톱 키와 같다).
/// 되감기를 끄면 되감기 버튼은 숨긴다.
private struct ActionButtons: View {
    @Environment(GameModel.self) private var model
    @Environment(\.padScale) private var scale

    var body: some View {
        VStack(spacing: 12) {
            HStack(alignment: .center, spacing: 16 * scale) {
                PadButton(label: "B", button: .b).padding(.top, 32 * scale)
                PadButton(label: "A", button: .a).padding(.bottom, 32 * scale)
            }
            HStack(spacing: 8) {
                if model.rewindEnabled {
                    HoldButton(label: "◀◀ 되감기", shape: .pill, width: 80, height: 28, onHold: model.setRewindTouch)
                }
                HoldButton(label: "▶▶ 배속", shape: .pill, width: 80, height: 28, onHold: model.setFastTouch)
            }
        }
    }
}

private enum KeyShape { case circle, pill, key }

private struct PadButton: View {
    @Environment(GameModel.self) private var model
    @Environment(\.padScale) private var scale
    let label: String
    let button: GbButton
    var shape: KeyShape = .circle

    var body: some View {
        let pill = shape == .pill
        HoldButton(
            label: label,
            shape: shape,
            width: (pill ? 72 : 64) * scale,
            height: (pill ? 28 : 64) * scale,
            color: button == .a || button == .b ? actionColor : keyColor
        ) { model.setButton(button, pressed: $0) }
    }
}

/// 누르는 동안 눌린 버튼. 버튼마다 따로 터치를 받으므로 방향키와 함께 누를 수 있다.
private struct HoldButton: View {
    @Environment(GameModel.self) private var model
    let label: String
    let shape: KeyShape
    let width: CGFloat
    let height: CGFloat
    var color: Color = keyColor
    let onHold: (Bool) -> Void
    @State private var held = false

    var body: some View {
        Text(label)
            .font(.caption.bold())
            .foregroundStyle(.white)
            .frame(width: width, height: height)
            .background(held ? color.opacity(0.7) : color)
            .clipShape(shape == .key ? AnyShape(RoundedRectangle(cornerRadius: 8)) : AnyShape(Capsule()))
            .accessibilityIdentifier("pad.\(label)")
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        guard !held else { return }
                        held = true
                        onHold(true)
                        if model.haptics { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
                    }
                    .onEnded { _ in
                        held = false
                        onHold(false)
                    }
            )
    }
}

/**
 방향키. 손가락을 떼지 않고 미끄러뜨려 방향을 바꿀 수 있고, 대각선도 누를 수 있다.
 가운데에서 가까우면 아무 방향도 누르지 않는다 (Android와 같은 판정).
 */
private struct DPad: View {
    @Environment(GameModel.self) private var model
    @Environment(\.padScale) private var scale
    let onButton: (GbButton, Bool) -> Void
    @State private var pressed: Set<GbButton> = []
    private var key: CGFloat { 52 * scale }

    var body: some View {
        ZStack {
            arrow("▲", .up).offset(y: -key)
            arrow("▼", .down).offset(y: key)
            arrow("◀", .left).offset(x: -key)
            arrow("▶", .right).offset(x: key)
        }
        .frame(width: key * 3, height: key * 3)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { press(directions($0.location)) }
                .onEnded { _ in press([]) }
        )
    }

    private func arrow(_ label: String, _ button: GbButton) -> some View {
        Text(label)
            .font(.caption)
            .foregroundStyle(.white)
            .frame(width: key, height: key)
            .background(pressed.contains(button) ? keyPressed : keyColor)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .accessibilityIdentifier("dpad.\(label)")
    }

    private func directions(_ location: CGPoint) -> Set<GbButton> {
        let size = key * 3
        let dx = location.x - size / 2
        let dy = location.y - size / 2
        // 대각선 판정 기준: 가운데 칸 절반보다 조금 작게
        let threshold = size / 6 * 0.75
        if abs(dx) < threshold && abs(dy) < threshold { return [] }
        var now = Set<GbButton>()
        // 한 축이 다른 축의 절반도 안 되면 그 축은 누르지 않는다 (의도치 않은 대각선 방지).
        if abs(dx) >= threshold && abs(dx) * 2 >= abs(dy) { now.insert(dx > 0 ? .right : .left) }
        if abs(dy) >= threshold && abs(dy) * 2 >= abs(dx) { now.insert(dy > 0 ? .down : .up) }
        return now
    }

    private func press(_ now: Set<GbButton>) {
        guard now != pressed else { return }
        pressed.subtracting(now).forEach { onButton($0, false) }
        let added = now.subtracting(pressed)
        added.forEach { onButton($0, true) }
        if model.haptics && !added.isEmpty { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
        pressed = now
    }
}

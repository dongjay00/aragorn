import SwiftUI

private let bodyColor = Color(red: 0x1E / 255, green: 0x1E / 255, blue: 0x24 / 255)
private let keyColor = Color(red: 0x3A / 255, green: 0x3A / 255, blue: 0x44 / 255)
private let keyPressed = Color(red: 0x5A / 255, green: 0x5A / 255, blue: 0x68 / 255)
private let actionColor = Color(red: 0x8B / 255, green: 0x22 / 255, blue: 0x52 / 255)

/// 게임 화면: 툴바, 게임 화면, 상태 줄, 가상 패드. 가로로 돌리면 방향키와 A/B가 양옆으로 간다.
struct GameView: View {
    @Environment(GameModel.self) private var model
    let onOpenRom: () -> Void
    @State private var slotsOpen = false

    var body: some View {
        GeometryReader { geometry in
            let landscape = geometry.size.width > geometry.size.height
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
                        .frame(maxWidth: .infinity)
                        VStack(spacing: 0) {
                            screen
                            status
                        }
                        .frame(width: (geometry.size.height - 96) * 160 / 144)
                        VStack {
                            Spacer()
                            ActionButtons()
                            Spacer()
                            PadButton(label: "START", button: .start, shape: .pill)
                        }
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
                    .padding(.bottom, 24)
                    HStack(spacing: 16) {
                        PadButton(label: "SELECT", button: .select, shape: .pill)
                        PadButton(label: "START", button: .start, shape: .pill)
                    }
                    .padding(.bottom, 24)
                }
            }
            .padding(.horizontal, 16)
        }
        .background(bodyColor.ignoresSafeArea())
        .preferredColorScheme(.dark)
        .sheet(isPresented: $slotsOpen) { SlotsView() }
    }

    private var toolbar: some View {
        HStack(spacing: 4) {
            Button("ROM", action: onOpenRom)
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
        } else {
            Rectangle()
                .fill(.black)
                .aspectRatio(160 / 144, contentMode: .fit)
                .overlay(Text("ROM을 열어 주세요").foregroundStyle(.gray))
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

/// A/B와 배속 버튼. 배속은 누르는 동안만 빨라진다 (데스크톱 배속 키와 같다).
private struct ActionButtons: View {
    @Environment(GameModel.self) private var model

    var body: some View {
        VStack(spacing: 12) {
            HStack(alignment: .center, spacing: 16) {
                PadButton(label: "B", button: .b).padding(.top, 32)
                PadButton(label: "A", button: .a).padding(.bottom, 32)
            }
            HoldButton(label: "▶▶ 배속", shape: .pill, width: 88, height: 28, onHold: model.setFastTouch)
        }
    }
}

private enum KeyShape { case circle, pill, key }

private struct PadButton: View {
    @Environment(GameModel.self) private var model
    let label: String
    let button: GbButton
    var shape: KeyShape = .circle

    var body: some View {
        let pill = shape == .pill
        HoldButton(
            label: label,
            shape: shape,
            width: pill ? 72 : 64,
            height: pill ? 28 : 64,
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
    let onButton: (GbButton, Bool) -> Void
    @State private var pressed: Set<GbButton> = []
    private let key: CGFloat = 52

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

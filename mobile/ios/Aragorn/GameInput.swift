import GameController

/// 키보드·게임패드 입력을 키 이름으로 바꿔 `onKey(device, name, pressed)`로 알린다.
/// 이름은 설정 파일에 그대로 저장된다.
/// - 키보드: `GCKeyCode`에 붙인 이름 (`UP`, `X`, `RETURN`, `F1`)
/// - 게임패드: Android와 같은 이름 (`DPAD_UP`, `BUTTON_A`, `BUTTON_R2`)
@MainActor
final class GameInput {
    /// 데스크톱처럼 F1–F10은 스테이트 단축키라 키 설정에 쓸 수 없다.
    static let slotKeys = (1...10).map { "F\($0)" }

    private let onKey: (InputDevice, String, Bool) -> Void
    /// 게임패드마다 지금 눌린 이름들. 바뀐 것만 알린다.
    private var pressed: [ObjectIdentifier: Set<String>] = [:]
    /// Shift+F1–F10(불러오기)을 알기 위해 Shift 상태를 기억한다.
    private(set) var shiftHeld = false

    init(onKey: @escaping (InputDevice, String, Bool) -> Void) {
        self.onKey = onKey
        let center = NotificationCenter.default
        center.addObserver(forName: .GCKeyboardDidConnect, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated { self?.attach(keyboard: note.object as? GCKeyboard) }
        }
        center.addObserver(forName: .GCControllerDidConnect, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated { self?.attach(controller: note.object as? GCController) }
        }
        center.addObserver(forName: .GCControllerDidDisconnect, object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated { self?.detach(controller: note.object as? GCController) }
        }
        attach(keyboard: GCKeyboard.coalesced)
        GCController.controllers().forEach { attach(controller: $0) }
    }

    private func attach(keyboard: GCKeyboard?) {
        keyboard?.keyboardInput?.keyChangedHandler = { [weak self] _, _, keyCode, isPressed in
            MainActor.assumeIsolated {
                guard let self else { return }
                if keyCode == .leftShift || keyCode == .rightShift { self.shiftHeld = isPressed }
                guard let name = Self.keyNames[keyCode] else { return }
                self.onKey(.keyboard, name, isPressed)
            }
        }
    }

    private func attach(controller: GCController?) {
        guard let controller, let gamepad = controller.extendedGamepad else { return }
        let id = ObjectIdentifier(controller)
        gamepad.valueChangedHandler = { [weak self] gamepad, _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                let now = Self.pressedNames(gamepad)
                let before = self.pressed[id] ?? []
                before.subtracting(now).forEach { self.onKey(.gamepad, $0, false) }
                now.subtracting(before).forEach { self.onKey(.gamepad, $0, true) }
                self.pressed[id] = now
            }
        }
    }

    /// 게임패드가 빠지면 누르고 있던 버튼을 뗀다.
    private func detach(controller: GCController?) {
        guard let controller else { return }
        let id = ObjectIdentifier(controller)
        pressed[id]?.forEach { onKey(.gamepad, $0, false) }
        pressed[id] = nil
    }

    private static func pressedNames(_ pad: GCExtendedGamepad) -> Set<String> {
        var names = Set<String>()
        let buttons: [(GCControllerButtonInput?, String)] = [
            (pad.dpad.up, "DPAD_UP"),
            (pad.dpad.down, "DPAD_DOWN"),
            (pad.dpad.left, "DPAD_LEFT"),
            (pad.dpad.right, "DPAD_RIGHT"),
            (pad.buttonA, "BUTTON_A"),
            (pad.buttonB, "BUTTON_B"),
            (pad.buttonX, "BUTTON_X"),
            (pad.buttonY, "BUTTON_Y"),
            (pad.leftShoulder, "BUTTON_L1"),
            (pad.rightShoulder, "BUTTON_R1"),
            (pad.leftTrigger, "BUTTON_L2"),
            (pad.rightTrigger, "BUTTON_R2"),
            (pad.buttonMenu, "BUTTON_START"),
            (pad.buttonOptions, "BUTTON_SELECT"),
            (pad.leftThumbstickButton, "BUTTON_THUMBL"),
            (pad.rightThumbstickButton, "BUTTON_THUMBR"),
        ]
        for (button, name) in buttons where button?.isPressed == true {
            names.insert(name)
        }
        return names
    }

    private static let keyNames: [GCKeyCode: String] = {
        var names: [GCKeyCode: String] = [
            .upArrow: "UP", .downArrow: "DOWN", .leftArrow: "LEFT", .rightArrow: "RIGHT",
            .returnOrEnter: "RETURN", .keypadEnter: "ENTER", .escape: "ESCAPE",
            .deleteOrBackspace: "DELETE", .tab: "TAB", .spacebar: "SPACE",
            .leftShift: "LEFT_SHIFT", .rightShift: "RIGHT_SHIFT",
            .leftControl: "LEFT_CONTROL", .rightControl: "RIGHT_CONTROL",
            .leftAlt: "LEFT_OPTION", .rightAlt: "RIGHT_OPTION",
            .hyphen: "MINUS", .equalSign: "EQUALS", .openBracket: "LEFT_BRACKET",
            .closeBracket: "RIGHT_BRACKET", .backslash: "BACKSLASH", .semicolon: "SEMICOLON",
            .quote: "QUOTE", .graveAccentAndTilde: "GRAVE", .comma: "COMMA",
            .period: "PERIOD", .slash: "SLASH",
        ]
        let letters: [GCKeyCode] = [
            .keyA, .keyB, .keyC, .keyD, .keyE, .keyF, .keyG, .keyH, .keyI, .keyJ, .keyK, .keyL, .keyM,
            .keyN, .keyO, .keyP, .keyQ, .keyR, .keyS, .keyT, .keyU, .keyV, .keyW, .keyX, .keyY, .keyZ,
        ]
        for (i, code) in letters.enumerated() {
            names[code] = String(UnicodeScalar(UInt8(65 + i)))
        }
        let digits: [GCKeyCode] = [.zero, .one, .two, .three, .four, .five, .six, .seven, .eight, .nine]
        for (i, code) in digits.enumerated() {
            names[code] = String(i)
        }
        let functions: [GCKeyCode] = [.F1, .F2, .F3, .F4, .F5, .F6, .F7, .F8, .F9, .F10, .F11, .F12]
        for (i, code) in functions.enumerated() {
            names[code] = "F\(i + 1)"
        }
        return names
    }()
}

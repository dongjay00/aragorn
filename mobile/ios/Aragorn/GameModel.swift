import Foundation
import Observation

/// 앱 화면. 닫으면 게임 화면으로 돌아온다.
enum Page { case game, settings, debugger }

/// 업데이트 확인 상태.
enum UpdateState: Equatable {
    case idle
    case checking
    case upToDate
    case failed(String)
    /// 나중에 해도 된다. 알림을 닫으면 `idle`로 돌아간다.
    case available(version: String, message: String)
    /// 업데이트 전까지 게임을 진행하지 않는다.
    case required(version: String, message: String)
}

/// 키 설정에서 입력을 기다리는 칸.
struct Capture: Equatable {
    let device: InputDevice
    let action: InputAction
}

/**
 앱 상태 전체: 설정, 실행 중인 게임, 입력 처리, 업데이트 확인 (Android `GameViewModel`과 같다).
 에뮬레이션은 Rust 스레드에서 돌고 여기서는 명령만 보낸다.
 */
@MainActor
@Observable
final class GameModel {
    let settings: Settings
    @ObservationIgnored private let updates = UpdateChecker.fromBundle()
    @ObservationIgnored private var input: GameInput?

    private(set) var emulator: Emulator?
    var page: Page = .game { didSet { capturing = nil } }
    /// 화면 아래에 보일 안내·오류 문구.
    private(set) var notice: String?
    private(set) var muted: Bool
    /// 가상 패드를 누를 때 햅틱을 낼지.
    private(set) var haptics: Bool
    /// 사용자가 일시정지했는지.
    private(set) var paused = false
    /// 지금 실행 배율 (`1배`, `4배`, `일시정지`).
    private(set) var speedText = ""
    private(set) var baseSpeed: Speed
    private(set) var fastSpeed: FastSpeed
    /// 키 설정이 바뀔 때마다 늘어난다. 설정 화면이 다시 읽는다.
    private(set) var bindingsVersion = 0
    private(set) var capturing: Capture?
    private(set) var slots: [SlotSummary] = []
    /// 디버거 브레이크포인트. ROM을 바꿔도 유지한다 (데스크톱과 같다).
    private(set) var breakpoints: [UInt16] = []
    /// 디버거의 채널별 음소거. ROM을 바꿔도 유지한다.
    private(set) var channelMuted = [false, false, false, false]
    private(set) var update: UpdateState = .idle
    var updateEnabled: Bool { updates.enabled }
    var releasePage: URL? { updates.releasePage }

    @ObservationIgnored private var visible = false
    @ObservationIgnored private var fastTouch = false
    @ObservationIgnored private var fastKey = false
    @ObservationIgnored private var lastAutoCheck: Date?
    private static let recheckInterval: TimeInterval = 6 * 60 * 60

    /// 세이브 폴더. 파일 앱의 "나의 iPhone › Aragorn › saves"에 보여 데스크톱과 주고받을 수 있다.
    nonisolated static let savesDirectory = URL.documentsDirectory.appending(path: "saves", directoryHint: .isDirectory)

    init() {
        let support = URL.applicationSupportDirectory
        try? FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
        settings = Settings.open(path: support.appending(path: "config.json").path)
        muted = settings.muted()
        haptics = settings.haptics()
        baseSpeed = settings.baseSpeed()
        fastSpeed = settings.fastSpeed()
        input = GameInput { [weak self] device, name, pressed in
            self?.onKey(device: device, name: name, pressed: pressed)
        }
        #if DEBUG
        applyLaunchArguments()
        #endif
        // 데스크톱처럼 6시간마다 확인한다. 화면에 보일 때만 확인한다.
        Task { [weak self] in
            while true {
                try? await Task.sleep(for: .seconds(Self.recheckInterval))
                guard let self else { return }
                if self.visible { self.autoCheck() }
            }
        }
    }

    #if DEBUG
    /// 개발용 실행 인자 (디버그 빌드만): `-rom <경로>`로 ROM을 바로 열고, `-page settings|debugger`로 화면을 고른다.
    /// 시뮬레이터에서 `xcrun simctl launch <기기> io.github.dongjay00.aragorn -rom <경로>`처럼 쓴다.
    private func applyLaunchArguments() {
        let defaults = UserDefaults.standard
        if let path = defaults.string(forKey: "rom") {
            openRom(URL(filePath: path))
        }
        let target: Page? = switch defaults.string(forKey: "page") {
        case "settings": .settings
        case "debugger": .debugger
        default: nil
        }
        guard let target else { return }
        // 디버거는 게임이 있어야 열린다. ROM이 열릴 때까지 기다린다.
        Task {
            for _ in 0..<50 where target == .debugger && emulator == nil {
                try? await Task.sleep(for: .milliseconds(100))
            }
            page = target
        }
    }
    #endif

    // MARK: ROM

    /// 파일 선택기로 고른 ROM을 연다. 세이브는 `Documents/saves/<ROM 파일 이름>` 옆에 둔다.
    func openRom(_ url: URL) {
        let previous = emulator
        Task {
            do {
                let (emulator, name) = try await Task.detached {
                    let accessing = url.startAccessingSecurityScopedResource()
                    defer { if accessing { url.stopAccessingSecurityScopedResource() } }
                    let rom = try Data(contentsOf: url)
                    let name = url.lastPathComponent.replacingOccurrences(of: "/", with: "_")
                    // 같은 ROM을 다시 열면 새 세션이 세이브를 읽기 전에 지금 진행을 저장해 둔다.
                    _ = previous?.flush()
                    let savePath = Self.savesDirectory.appending(path: name).path
                    return (try Emulator(rom: rom, savePath: savePath), name)
                }.value
                previous?.stop()
                configure(emulator)
                self.emulator = emulator
                // 헤더에 제목이 없는 ROM(테스트 ROM 등)은 파일 이름을 보인다.
                let title = emulator.title()
                notice = "\(title.isEmpty ? name : title) 실행 중"
                refreshSlots()
            } catch {
                notice = Self.message(error) ?? "ROM을 열 수 없습니다"
            }
        }
    }

    /// 새 에뮬레이터에 지금 설정과 디버거 상태를 적용한다.
    private func configure(_ emulator: Emulator) {
        emulator.setSpeed(base: baseSpeed, fast: fastSpeed)
        emulator.setMuted(muted: muted)
        emulator.setFastHeld(held: fastTouch || fastKey)
        emulator.setBreakpoints(pcs: breakpoints)
        for (channel, muted) in channelMuted.enumerated() {
            emulator.setChannelMuted(channel: UInt8(channel), muted: muted)
        }
        if case .required = update { emulator.setBlocked(blocked: true) }
        emulator.setBackground(background: !visible)
        paused = false
    }

    // MARK: 화면

    /// 앱이 화면에 보이는지. 백그라운드로 가면 멈추고 Rust 쪽이 세이브를 저장한다.
    func setVisible(_ value: Bool) {
        visible = value
        emulator?.setBackground(background: !value)
        // 앱을 처음 열 때, 그리고 오래 백그라운드에 있다가 돌아왔을 때 확인한다.
        if value, lastAutoCheck.map({ Date().timeIntervalSince($0) >= Self.recheckInterval }) ?? true {
            autoCheck()
        }
    }

    /// 화면 갱신마다 부른다. 에뮬레이터의 안내 문구와 실행 상태를 옮겨 온다.
    func poll() {
        guard let emulator else { return }
        if let last = emulator.takeNotices().last { notice = last }
        let speed = emulator.speedText()
        if speed != speedText { speedText = speed }
        let isPaused = emulator.isPaused()
        if isPaused != paused { paused = isPaused }
    }

    // MARK: 입력

    /// 화면의 가상 패드.
    func setButton(_ button: GbButton, pressed: Bool) {
        emulator?.setButton(button: button, pressed: pressed)
    }

    /// 화면의 배속 버튼 (누르는 동안).
    func setFastTouch(_ held: Bool) {
        fastTouch = held
        emulator?.setFastHeld(held: fastTouch || fastKey)
    }

    /// 키보드·게임패드 입력. 키 설정 중이면 그 칸에 지정하고, 아니면 지정된 기능을 실행한다.
    func onKey(device: InputDevice, name: String, pressed: Bool) {
        if device == .keyboard, let slot = GameInput.slotKeys.firstIndex(of: name) {
            if pressed { onSlotKey(slot, load: input?.shiftHeld ?? false) }
            return
        }
        if let capture = capturing {
            if pressed, device == capture.device { bind(device: device, action: capture.action, name: name) }
            return
        }
        guard page == .game else { return }
        for action in settings.actionsFor(device: device, name: name) {
            if let button = actionButton(action: action) {
                setButton(button, pressed: pressed)
            } else if action == .fastForward {
                fastKey = pressed
                emulator?.setFastHeld(held: fastTouch || fastKey)
            } else if pressed, action == .pause {
                togglePause()
            } else if pressed, action == .mute {
                setMuted(!muted)
            }
        }
    }

    /// 키보드 F1–F10: 스테이트 저장, Shift를 함께 누르면 불러오기 (데스크톱과 같다).
    private func onSlotKey(_ slot: Int, load: Bool) {
        if capturing != nil {
            notice = "F1–F10은 스테이트 단축키라 지정할 수 없습니다"
            return
        }
        guard page == .game else { return }
        if load { loadState(slot) } else { saveState(slot) }
    }

    // MARK: 실행

    func togglePause() {
        guard let emulator else { return }
        paused = emulator.togglePause()
    }

    func setMuted(_ value: Bool) {
        muted = value
        emulator?.setMuted(muted: value)
        saveSetting { try self.settings.setMuted(muted: value) }
    }

    func setHaptics(_ value: Bool) {
        haptics = value
        saveSetting { try self.settings.setHaptics(on: value) }
    }

    func setBaseSpeed(_ speed: Speed) {
        baseSpeed = speed
        emulator?.setSpeed(base: speed, fast: fastSpeed)
        saveSetting { try self.settings.setBaseSpeed(speed: speed) }
    }

    func setFastSpeed(_ speed: FastSpeed) {
        fastSpeed = speed
        emulator?.setSpeed(base: baseSpeed, fast: speed)
        saveSetting { try self.settings.setFastSpeed(speed: speed) }
    }

    // MARK: 스테이트

    func saveState(_ slot: Int) {
        runSlot("슬롯 \(slot + 1)에 저장했습니다") { try $0.saveState(slot: UInt8(slot)) }
    }

    func loadState(_ slot: Int) {
        runSlot("슬롯 \(slot + 1)을(를) 불러왔습니다") { try $0.loadState(slot: UInt8(slot)) }
    }

    /// 슬롯 목록(썸네일, 저장 시각)을 다시 읽는다. 슬롯 화면을 열 때 부른다.
    func refreshSlots() {
        guard let emulator else {
            slots = []
            return
        }
        Task {
            slots = await Task.detached { emulator.slots() }.value
        }
    }

    private func runSlot(_ done: String, _ action: @escaping @Sendable (Emulator) throws -> Void) {
        guard let emulator else { return }
        Task {
            let result = await Task.detached { () -> String? in
                do {
                    try action(emulator)
                    return done
                } catch {
                    return Self.message(error)
                }
            }.value
            notice = result
            refreshSlots()
        }
    }

    // MARK: 키 설정

    /// 칸을 누르면 입력을 기다리고, 같은 칸을 다시 누르면 그만둔다.
    func toggleCapture(device: InputDevice, action: InputAction) {
        let capture = Capture(device: device, action: action)
        capturing = capturing == capture ? nil : capture
    }

    func cancelCapture() {
        capturing = nil
    }

    /// 입력을 기다리는 칸의 지정을 지운다.
    func clearCaptured() {
        guard let capture = capturing else { return }
        bind(device: capture.device, action: capture.action, name: "")
    }

    func resetInput() {
        capturing = nil
        saveSetting { try self.settings.resetInput() }
        baseSpeed = settings.baseSpeed()
        fastSpeed = settings.fastSpeed()
        emulator?.setSpeed(base: baseSpeed, fast: fastSpeed)
        bindingsVersion += 1
    }

    private func bind(device: InputDevice, action: InputAction, name: String) {
        capturing = nil
        saveSetting { try self.settings.bind(device: device, action: action, name: name) }
        bindingsVersion += 1
    }

    private func saveSetting(_ change: () throws -> Void) {
        do {
            try change()
        } catch {
            notice = Self.message(error)
        }
    }

    // MARK: 디버거

    func toggleBreakpoint(_ pc: UInt16) {
        if let index = breakpoints.firstIndex(of: pc) {
            breakpoints.remove(at: index)
        } else {
            breakpoints.append(pc)
            breakpoints.sort()
        }
        emulator?.setBreakpoints(pcs: breakpoints)
    }

    func setChannelMuted(_ channel: Int, _ value: Bool) {
        channelMuted[channel] = value
        emulator?.setChannelMuted(channel: UInt8(channel), muted: value)
    }

    // MARK: 업데이트

    private func autoCheck() {
        lastAutoCheck = Date()
        checkForUpdate(manual: false)
    }

    /// `manual`이면 사용자가 누른 것이라 결과를 안내 문구로도 보인다.
    func checkForUpdate(manual: Bool) {
        guard updates.enabled else {
            if manual { notice = "이 빌드는 업데이트를 확인하지 않습니다" }
            return
        }
        switch update {
        case .checking, .required: return
        default: break
        }
        update = .checking
        let skipped = settings.skippedVersion()
        Task {
            do {
                switch try await updates.check(skippedVersion: skipped) {
                case .upToDate: update = .upToDate
                case let .available(version, message): update = .available(version: version, message: message)
                case let .required(version, message): update = .required(version: version, message: message)
                }
            } catch {
                update = .failed(Self.message(error) ?? "업데이트를 확인하지 못했습니다")
            }
            if case .required = update { emulator?.setBlocked(blocked: true) }
            if manual {
                switch update {
                case .upToDate: notice = "최신 버전입니다"
                case let .failed(reason): notice = reason
                default: break
                }
            }
        }
    }

    /// "나중에": 다음 확인 때 다시 알린다.
    func dismissUpdate() {
        if case .available = update { update = .idle }
    }

    /// "이 버전 건너뛰기": 이 버전은 다시 알리지 않는다.
    func skipUpdate() {
        guard case let .available(version, _) = update else { return }
        saveSetting { try self.settings.setSkippedVersion(version: version) }
        update = .idle
    }

    /// 사용자에게 보일 문구. 바인딩이 만든 설명은 `Rom(reason: ...)` 형식이라 그대로 쓰지 않는다.
    nonisolated static func message(_ error: Error) -> String? {
        switch error {
        case let EmulatorError.Rom(reason), let EmulatorError.State(reason),
             let EmulatorError.Io(reason), let EmulatorError.Update(reason):
            return reason
        case EmulatorError.Stopped:
            return "에뮬레이터가 이미 꺼졌습니다"
        default:
            return error.localizedDescription
        }
    }
}

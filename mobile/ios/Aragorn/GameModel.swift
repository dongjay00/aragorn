import Foundation
import Observation
import Photos

/// 앱 화면. 닫으면 게임 화면으로 돌아온다.
enum Page { case game, settings, debugger, cheats }

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
    /// 화면 필터 (팔레트, 색 보정, LCD 격자).
    private(set) var video: VideoSettings
    private(set) var rewindEnabled: Bool
    /// 게임을 닫을 때의 자리에서 이어서 할지.
    private(set) var autoResume: Bool
    /// 가상 패드 크기·불투명도·높이.
    var padLayout: PadLayout
    /// 최근에 연 게임, 최근 것부터.
    private(set) var recent: [RecentRom] = []
    /// 지금 게임의 치트 목록.
    private(set) var cheats: [CheatInfo] = []
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
    /// 지금 게임의 ROM 파일 ("처음부터 다시 시작"에 쓴다).
    @ObservationIgnored private var currentRom: URL?
    @ObservationIgnored private var rewindTouch = false
    @ObservationIgnored private var rewindKey = false
    @ObservationIgnored private var lastAutoCheck: Date?
    private static let recheckInterval: TimeInterval = 6 * 60 * 60

    /// 세이브 폴더. 파일 앱의 "나의 iPhone › Aragorn › saves"에 보여 데스크톱과 주고받을 수 있다.
    nonisolated static let savesDirectory = URL.documentsDirectory.appending(path: "saves", directoryHint: .isDirectory)
    /// 연 ROM을 복사해 두는 폴더. 최근 게임을 다시 열 때 쓴다. 파일 앱에서 ROM을 직접 넣을 수도 있다.
    nonisolated static let romsDirectory = URL.documentsDirectory.appending(path: "roms", directoryHint: .isDirectory)

    init() {
        let support = URL.applicationSupportDirectory
        try? FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
        settings = Settings.open(path: support.appending(path: "config.json").path)
        muted = settings.muted()
        haptics = settings.haptics()
        baseSpeed = settings.baseSpeed()
        fastSpeed = settings.fastSpeed()
        video = settings.video()
        rewindEnabled = settings.rewindEnabled()
        autoResume = settings.autoResume()
        padLayout = settings.padLayout()
        // 지운 ROM이나 예전 앱 폴더를 가리키는 항목은 뺀다.
        for game in settings.recentGames()
        where !FileManager.default.fileExists(atPath: Self.recentURL(game.path).path) {
            try? settings.removeRecent(path: game.path)
        }
        recent = settings.recentGames()
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
        case "cheats": .cheats
        default: nil
        }
        guard let target else { return }
        // 디버거와 치트는 게임이 있어야 열린다. ROM이 열릴 때까지 기다린다.
        Task {
            for _ in 0..<50 where target != .settings && emulator == nil {
                try? await Task.sleep(for: .milliseconds(100))
            }
            page = target
        }
    }
    #endif

    // MARK: ROM

    /// 파일 선택기로 고른 ROM을 연다. 최근 게임으로 다시 열 수 있게 `Documents/roms/`에 복사해 두고,
    /// 세이브는 `Documents/saves/<ROM 파일 이름>` 옆에 둔다.
    func openRom(_ url: URL) {
        startGame {
            let accessing = url.startAccessingSecurityScopedResource()
            defer { if accessing { url.stopAccessingSecurityScopedResource() } }
            let rom = try Data(contentsOf: url)
            let name = url.lastPathComponent.replacingOccurrences(of: "/", with: "_")
            let copy = Self.romsDirectory.appending(path: name)
            // 이미 roms/ 안의 파일을 고른 경우에는 그대로 쓴다.
            if copy.standardizedFileURL != url.standardizedFileURL {
                try FileManager.default.createDirectory(at: Self.romsDirectory, withIntermediateDirectories: true)
                try rom.write(to: copy, options: .atomic)
            }
            return (rom, copy)
        }
    }

    /// 최근 게임을 다시 연다. 복사해 둔 ROM이 없으면 목록에서 뺀다.
    func openRecent(_ path: String) {
        let url = Self.recentURL(path)
        guard FileManager.default.fileExists(atPath: url.path) else {
            saveSetting { try self.settings.removeRecent(path: path) }
            recent = settings.recentGames()
            notice = "ROM 파일이 없어 최근 목록에서 뺐습니다"
            return
        }
        startGame {
            (try Data(contentsOf: url), url)
        }
    }

    /// 최근 목록에는 `Documents` 기준 상대 경로(`roms/<이름>`)를 저장한다. 앱을 다시 설치하거나 업데이트하면
    /// 앱 폴더의 절대 경로가 바뀌기 때문이다.
    nonisolated static func recentPath(_ url: URL) -> String {
        let documents = URL.documentsDirectory.standardizedFileURL.path + "/"
        let path = url.standardizedFileURL.path
        return path.hasPrefix(documents) ? String(path.dropFirst(documents.count)) : path
    }

    nonisolated static func recentURL(_ path: String) -> URL {
        path.hasPrefix("/") ? URL(filePath: path) : URL.documentsDirectory.appending(path: path)
    }

    /// 지금 게임을 이어하기 없이 게임 세이브에서 다시 시작한다.
    func restart() {
        guard let file = currentRom else { return }
        startGame(resume: false) { (try Data(contentsOf: file), file) }
    }

    /// `load`가 읽은 ROM으로 게임을 시작하고 최근 목록 맨 앞에 넣는다.
    /// `resume`이면 이어하기를 켰을 때 지난번에 닫은 자리에서 이어서 한다.
    private func startGame(resume: Bool = true, _ load: @escaping @Sendable () throws -> (Data, URL)) {
        let previous = emulator
        let useResume = resume && autoResume
        Task {
            do {
                let (emulator, file, resumed) = try await Task.detached {
                    let (rom, file) = try load()
                    // 지금 게임은 닫히면서 이어하기를 다시 남기지 않게 한다. 새 세션이 이어하기를 읽은 뒤에
                    // 덮어쓰면 안 된다. 다른 게임으로 바꿀 때는 지금 자리를 먼저 남긴다.
                    if let previous {
                        previous.setAutoResume(on: false)
                        if useResume { try? previous.saveResume() }
                        // 같은 ROM을 다시 열면 새 세션이 세이브를 읽기 전에 지금 진행을 저장해 둔다.
                        _ = previous.flush()
                    }
                    let savePath = Self.savesDirectory.appending(path: file.lastPathComponent).path
                    let emulator = try Emulator(rom: rom, savePath: savePath)
                    let resumed: Result<Bool, Error> = useResume
                        ? Result { try emulator.resume() }
                        : .success(false)
                    return (emulator, file, resumed)
                }.value
                previous?.stop()
                configure(emulator)
                self.emulator = emulator
                // 헤더에 제목이 없는 ROM(테스트 ROM 등)은 파일 이름을 보인다.
                currentRom = file
                let name = file.lastPathComponent
                let title = emulator.title().isEmpty ? name : emulator.title()
                switch resumed {
                case .success(true): notice = "\(title) — 지난번에 닫은 자리에서 이어서 합니다"
                case .success(false): notice = "\(title) 실행 중"
                case let .failure(error): notice = Self.message(error) ?? "\(title) 실행 중"
                }
                saveSetting { try self.settings.addRecent(path: Self.recentPath(file), title: title) }
                recent = settings.recentGames()
                cheats = emulator.cheats()
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
        emulator.setRewindHeld(held: rewindTouch || rewindKey)
        emulator.setRewindEnabled(enabled: rewindEnabled)
        emulator.setAutoResume(on: autoResume)
        emulator.setVideo(video: video)
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

    /// 화면의 되감기 버튼 (누르는 동안).
    func setRewindTouch(_ held: Bool) {
        rewindTouch = held
        emulator?.setRewindHeld(held: rewindTouch || rewindKey)
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
            } else if action == .rewind {
                rewindKey = pressed
                emulator?.setRewindHeld(held: rewindTouch || rewindKey)
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

    func setVideo(_ value: VideoSettings) {
        video = value
        emulator?.setVideo(video: value)
        saveSetting { try self.settings.setVideo(video: value) }
    }

    func setAutoResume(_ value: Bool) {
        autoResume = value
        emulator?.setAutoResume(on: value)
        saveSetting { try self.settings.setAutoResume(on: value) }
    }

    /// 지금 화면을 사진 앱에 저장한다. 처음에는 사진 추가 권한을 묻는다.
    func saveScreenshot() {
        guard let emulator else { return }
        Task {
            do {
                let png = try await Task.detached { try emulator.screenshot() }.value
                guard await PHPhotoLibrary.requestAuthorization(for: .addOnly) == .authorized else {
                    notice = "사진 접근을 허용하지 않아 스크린샷을 저장하지 못했습니다"
                    return
                }
                try await PHPhotoLibrary.shared().performChanges {
                    PHAssetCreationRequest.forAsset().addResource(with: .photo, data: png, options: nil)
                }
                notice = "스크린샷을 사진에 저장했습니다"
            } catch {
                notice = "스크린샷을 저장하지 못했습니다: \(Self.message(error) ?? "")"
            }
        }
    }

    func setRewindEnabled(_ value: Bool) {
        rewindEnabled = value
        emulator?.setRewindEnabled(enabled: value)
        saveSetting { try self.settings.setRewindEnabled(on: value) }
    }

    /// 슬라이더를 놓았을 때 부른다. 움직이는 동안에는 `padLayout`만 바꾼다.
    func savePadLayout() {
        saveSetting { try self.settings.setPadLayout(layout: self.padLayout) }
        padLayout = settings.padLayout()
    }

    func resetPadLayout() {
        padLayout = PadLayout(scale: 1, opacity: 1, lift: 0)
        savePadLayout()
    }

    // MARK: 치트

    /// 더했으면 `true`. 코드가 틀리면 안내 문구로 이유를 보인다.
    @discardableResult
    func addCheat(code: String, name: String) -> Bool {
        let ok = cheatChange { try $0.addCheat(code: code, name: name) }
        if ok { notice = "치트를 켰습니다" }
        return ok
    }

    func setCheatEnabled(_ index: Int, _ enabled: Bool) {
        cheatChange { try $0.setCheatEnabled(index: UInt32(index), enabled: enabled) }
    }

    func removeCheat(_ index: Int) {
        cheatChange { try $0.removeCheat(index: UInt32(index)) }
    }

    @discardableResult
    private func cheatChange(_ change: (Emulator) throws -> Void) -> Bool {
        guard let emulator else { return false }
        var ok = true
        do {
            try change(emulator)
        } catch {
            notice = Self.message(error)
            ok = false
        }
        cheats = emulator.cheats()
        return ok
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

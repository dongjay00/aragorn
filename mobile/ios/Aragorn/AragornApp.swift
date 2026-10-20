import AVFAudio
import SwiftUI
import UniformTypeIdentifiers

@main
struct AragornApp: App {
    @State private var model = GameModel()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        // 게임 소리: 무음 스위치를 따르고, 다른 앱의 음악과 섞이지 않는다.
        try? AVAudioSession.sharedInstance().setCategory(.soloAmbient)
        try? AVAudioSession.sharedInstance().setActive(true)
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .preferredColorScheme(.dark)
        }
        .onChange(of: scenePhase, initial: true) { _, phase in
            // 백그라운드로 가면 멈추고 Rust 쪽이 세이브를 저장한다.
            model.setVisible(phase == .active)
        }
    }
}

private struct RootView: View {
    @Environment(GameModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var importing = false

    var body: some View {
        Group {
            switch model.page {
            case .game: GameView(onOpenRom: { importing = true })
            case .settings: SettingsView()
            case .debugger: DebuggerView()
            case .cheats: CheatsView()
            }
        }
        // .gb/.gbc에는 등록된 파일 형식이 없어 모든 파일을 보여 준다.
        .fileImporter(isPresented: $importing, allowedContentTypes: [.data]) { result in
            if case let .success(url) = result { model.openRom(url) }
        }
        .alert(availableTitle, isPresented: availableShown) {
            Button("받으러 가기") {
                download()
                model.dismissUpdate()
            }
            Button("이 버전 건너뛰기", action: model.skipUpdate)
            Button("나중에", role: .cancel, action: model.dismissUpdate)
        } message: {
            if case let .available(_, message) = model.update {
                Text(message.isEmpty ? "새 버전이 나왔습니다." : message)
            }
        }
        // 강제 업데이트는 닫을 수 없고 게임도 멈춘다 (데스크톱과 같다).
        .alert("업데이트가 필요합니다", isPresented: requiredShown) {
            Button("받으러 가기", action: download)
        } message: {
            if case let .required(version, message) = model.update {
                Text(
                    "이 버전은 더 이상 지원하지 않습니다. \(version)으로 업데이트해 주세요."
                        + (message.isEmpty ? "" : "\n\n\(message)")
                        + "\n\n게임 세이브는 저장해 두었습니다."
                )
            }
        }
    }

    private var availableTitle: String {
        if case let .available(version, _) = model.update { return "새 버전 \(version)" }
        return ""
    }

    private var availableShown: Binding<Bool> {
        Binding(
            get: { if case .available = model.update { true } else { false } },
            set: { if !$0 { model.dismissUpdate() } }
        )
    }

    private var requiredShown: Binding<Bool> {
        // 닫아도 다시 뜬다.
        Binding(get: { if case .required = model.update { true } else { false } }, set: { _ in })
    }

    private func download() {
        if let page = model.releasePage { openURL(page) }
    }
}

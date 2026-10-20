import SwiftUI

/// 데스크톱 설정 창과 같은 항목(키·게임패드 매핑, 속도, 화면, 되감기, 음소거, 업데이트)과 가상 패드 모양.
struct SettingsView: View {
    @Environment(GameModel.self) private var model

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 6) {
                        GridRow {
                            Text("기능").bold()
                            Text("키보드").bold()
                            Text("게임패드").bold()
                        }
                        ForEach(allActions(), id: \.self) { action in
                            GridRow {
                                Text(actionLabel(action: action))
                                cell(.keyboard, action)
                                cell(.gamepad, action)
                            }
                        }
                    }
                    // 키 설정이 바뀌면 다시 읽는다.
                    .id(model.bindingsVersion)
                    if model.capturing != nil {
                        HStack {
                            Text("키를 누르세요")
                            Spacer()
                            Button("지우기", action: model.clearCaptured)
                            Button("취소", action: model.cancelCapture)
                        }
                        .buttonStyle(.borderless)
                    }
                } header: {
                    Text("키 설정")
                } footer: {
                    Text("칸을 누른 뒤 키보드나 게임패드 버튼을 누르세요. F1–F10은 스테이트 단축키라 지정할 수 없습니다.")
                }

                Section {
                    Picker("기본 속도", selection: Binding(get: { model.baseSpeed }, set: model.setBaseSpeed)) {
                        ForEach([Speed.x1, .x2, .x4], id: \.self) { Text(speedLabel(speed: $0)).tag($0) }
                    }
                    Picker("배속 속도", selection: Binding(get: { model.fastSpeed }, set: model.setFastSpeed)) {
                        ForEach([FastSpeed.x2, .x4, .unlimited], id: \.self) {
                            Text(fastSpeedLabel(speed: $0)).tag($0)
                        }
                    }
                } header: {
                    Text("속도")
                } footer: {
                    Text("배속 속도는 배속 버튼·키를 누르는 동안의 속도입니다. 2배·4배에서는 소리가 빠르고 높게 들립니다. 무제한에서는 소리가 나지 않습니다.")
                }

                Section {
                    Toggle("이어하기", isOn: Binding(get: { model.autoResume }, set: model.setAutoResume))
                } header: {
                    Text("이어하기")
                } footer: {
                    Text("앱을 닫거나 다른 게임을 열 때의 자리를 남기고, 다시 열면 그 자리에서 이어서 합니다. 메뉴의 \"처음부터 다시 시작\"으로 게임 세이브에서 시작할 수 있습니다.")
                }

                Section {
                    Toggle("되감기", isOn: Binding(get: { model.rewindEnabled }, set: model.setRewindEnabled))
                } header: {
                    Text("되감기")
                } footer: {
                    Text("되감기 버튼·키를 누르는 동안 최근 약 20초를 거꾸로 돌립니다. 끄면 메모리를 아낍니다.")
                }

                Section {
                    Picker("흑백 게임 색", selection: videoBinding(\.palette)) {
                        ForEach(allPalettes(), id: \.self) { Text(paletteLabel(palette: $0)).tag($0) }
                    }
                    Toggle("컬러 게임 색 보정", isOn: videoBinding(\.colorCorrection))
                    Toggle("LCD 격자", isOn: videoBinding(\.lcdGrid))
                } header: {
                    Text("화면")
                } footer: {
                    Text("색 보정은 게임보이 컬러 화면처럼 색을 덜 진하게 합니다. LCD 격자는 픽셀 사이에 선을 그립니다.")
                }

                Section {
                    padSlider("크기", \.scale, 0.7...1.4)
                    padSlider("불투명도", \.opacity, 0.2...1)
                    padSlider("높이", \.lift, 0...1)
                    Button("가상 패드를 기본값으로", action: model.resetPadLayout)
                } header: {
                    Text("가상 패드")
                } footer: {
                    Text("화면에 들어가지 않을 만큼 크게 하면 들어가는 만큼만 커집니다.")
                }

                Section {
                    Toggle("음소거", isOn: Binding(get: { model.muted }, set: model.setMuted))
                    Toggle("버튼 햅틱", isOn: Binding(get: { model.haptics }, set: model.setHaptics))
                } header: {
                    Text("소리와 햅틱")
                } footer: {
                    Text("버튼 햅틱: 화면의 방향키와 버튼을 누를 때 진동합니다.")
                }

                Section("세이브") {
                    Text("파일 앱의 \"나의 iPhone › Aragorn › saves\"에 데스크톱과 같은 형식(.sav, .ss0–.ss9)으로 저장합니다. 데스크톱의 세이브를 ROM 파일 이름에 맞춰 넣으면 이어서 할 수 있습니다.")
                        .font(.footnote)
                }

                Section("업데이트") {
                    LabeledContent("현재 버전", value: appVersion())
                    if let text = updateText, !text.isEmpty {
                        Text(text).font(.footnote)
                    }
                    Button("업데이트 확인") { model.checkForUpdate(manual: true) }
                        .disabled(model.update == .checking)
                }

                Section {
                    Button("키 설정과 속도를 기본값으로", action: model.resetInput)
                }

                if let notice = model.notice {
                    Text(notice).font(.footnote).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("설정")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("돌아가기") { model.page = .game }
                }
            }
        }
    }

    private func videoBinding<T>(_ key: WritableKeyPath<VideoSettings, T>) -> Binding<T> {
        Binding(
            get: { model.video[keyPath: key] },
            set: {
                var video = model.video
                video[keyPath: key] = $0
                model.setVideo(video)
            }
        )
    }

    /// 움직이는 동안에는 화면에만 반영하고, 놓으면 저장한다.
    private func padSlider(
        _ label: String, _ key: WritableKeyPath<PadLayout, Float>, _ range: ClosedRange<Float>
    ) -> some View {
        LabeledContent(label) {
            HStack {
                Slider(
                    value: Binding(get: { model.padLayout[keyPath: key] }, set: { model.padLayout[keyPath: key] = $0 }),
                    in: range
                ) { editing in
                    if !editing { model.savePadLayout() }
                }
                Text("\(Int((model.padLayout[keyPath: key] * 100).rounded()))%")
                    .font(mono)
                    .frame(width: 44, alignment: .trailing)
            }
        }
        .accessibilityIdentifier("pad.slider.\(label)")
    }

    private func cell(_ device: InputDevice, _ action: InputAction) -> some View {
        let waiting = model.capturing == Capture(device: device, action: action)
        let name = model.settings.binding(device: device, action: action)
        return Button {
            model.toggleCapture(device: device, action: action)
        } label: {
            Text(waiting ? "입력 대기…" : name.isEmpty ? "없음" : name)
                .font(mono)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
                .background(waiting ? Color.accentColor.opacity(0.3) : .clear, in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.borderless)
    }

    private var updateText: String? {
        guard model.updateEnabled else { return "이 빌드는 업데이트를 확인하지 않습니다" }
        switch model.update {
        case .idle: return nil
        case .checking: return "확인하는 중…"
        case .upToDate: return "최신 버전입니다"
        case let .failed(reason): return "확인하지 못했습니다: \(reason)"
        case let .available(version, _): return "새 버전 \(version)이(가) 있습니다"
        case let .required(version, _): return "새 버전 \(version)으로 업데이트해야 합니다"
        }
    }
}

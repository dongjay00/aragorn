import SwiftUI

/// 링크 케이블: 같은 와이파이의 다른 Aragorn과 통신 교환·대전을 한다.
struct LinkView: View {
    @Environment(GameModel.self) private var model
    @State private var state: LinkState?
    @State private var address = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    switch state {
                    case let .waiting(address):
                        Text("상대를 기다리는 중").bold()
                        LabeledContent("이 기기 주소", value: address)
                            .accessibilityIdentifier("link.address")
                        Button("그만두기", action: model.linkDisconnect)
                    case let .connecting(address):
                        Text("\(address)에 연결하는 중…")
                        Button("그만두기", action: model.linkDisconnect)
                    case let .connected(peer):
                        Text("\(peer)와 연결됨").bold().accessibilityIdentifier("link.connected")
                        Text("게임 안에서 통신 교환이나 대전을 시작하세요. 교환·대전 중에는 되감기를 쓸 수 없습니다.")
                            .font(.footnote)
                        Button("끊기", action: model.linkDisconnect)
                    case nil:
                        Button("기다리기", action: model.linkHost)
                        TextField("상대 주소 (192.168.0.5)", text: $address)
                            .keyboardType(.numbersAndPunctuation)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .accessibilityIdentifier("link.peer")
                        Button("연결") { model.linkConnect(address) }
                            .disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                } footer: {
                    Text("같은 와이파이의 다른 Aragorn(폰이나 PC)과 통신 교환·대전을 합니다. 한쪽이 \"기다리기\"를 누르고, 다른 쪽이 보이는 주소로 연결하세요.")
                }
                if let notice = model.notice {
                    Text(notice).font(.footnote).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("링크 케이블")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("돌아가기") { model.page = .game }
                }
            }
            // 연결 상태는 다른 스레드에서 바뀌므로 자주 다시 읽는다.
            .task {
                while !Task.isCancelled {
                    let emulator = model.emulator
                    state = await Task.detached { emulator?.linkState() }.value
                    try? await Task.sleep(for: .milliseconds(300))
                }
            }
        }
    }
}

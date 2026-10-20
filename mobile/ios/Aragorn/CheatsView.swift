import SwiftUI

/// 지금 게임의 치트 목록. 게임 지니·게임샤크 코드를 더하고 켜고 끈다. 목록은 세이브 옆 `.cht`에 저장된다.
struct CheatsView: View {
    @Environment(GameModel.self) private var model
    @State private var code = ""
    @State private var name = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("치트") {
                    if model.cheats.isEmpty {
                        Text("아직 치트가 없습니다.").foregroundStyle(.secondary)
                    }
                    ForEach(Array(model.cheats.enumerated()), id: \.offset) { index, cheat in
                        Toggle(isOn: Binding(get: { cheat.enabled }, set: { model.setCheatEnabled(index, $0) })) {
                            VStack(alignment: .leading) {
                                Text(cheat.label).lineLimit(1)
                                if cheat.label != cheat.code {
                                    Text(cheat.code).font(mono).foregroundStyle(.secondary)
                                }
                            }
                        }
                        .accessibilityIdentifier("cheat.\(index)")
                    }
                    .onDelete { offsets in
                        // 뒤에서부터 지워야 앞의 번호가 바뀌지 않는다.
                        for index in offsets.sorted(by: >) { model.removeCheat(index) }
                    }
                }

                Section {
                    TextField("코드 (00A-17B-C49 또는 010138CD)", text: $code, axis: .vertical)
                        .font(mono)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("cheat.code")
                    TextField("이름 (선택)", text: $name)
                        .accessibilityIdentifier("cheat.name")
                    Button("더하기") {
                        if model.addCheat(code: code, name: name) {
                            code = ""
                            name = ""
                        }
                    }
                    .disabled(code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                } header: {
                    Text("새 치트")
                } footer: {
                    Text("게임 지니(ABC-DEF-GHI)와 게임샤크(01VVAAAA) 코드를 씁니다. 여러 코드를 줄을 바꿔 쓰거나 + 로 이으면 한 치트로 켜고 끕니다. 밀어서 지울 수 있습니다.")
                }

                if let notice = model.notice {
                    Text(notice).font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("cheat.notice")
                }
            }
            .navigationTitle("치트")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("돌아가기") { model.page = .game }
                }
            }
        }
    }
}

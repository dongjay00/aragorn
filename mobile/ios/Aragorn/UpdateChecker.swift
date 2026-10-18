import Foundation

/// GitHub Releases의 latest 릴리스에서 서명된 업데이트 정책(데스크톱과 같은 파일)을 받아 Rust에서 검증한다.
/// 정책에 `ios` 패키지가 있을 때만 새 버전을 알린다.
struct UpdateChecker {
    let repo: String

    /// 이 빌드가 업데이트를 확인하는지. 릴리스 빌드만 저장소를 안다.
    var enabled: Bool { !repo.isEmpty }

    var releasePage: URL? { URL(string: "https://github.com/\(repo)/releases/latest") }

    /// Info.plist의 `AragornRepo` (빌드 설정 ARAGORN_REPO).
    static func fromBundle() -> UpdateChecker {
        UpdateChecker(repo: Bundle.main.object(forInfoDictionaryKey: "AragornRepo") as? String ?? "")
    }

    func check(skippedVersion: String?) async throws -> UpdateCheck {
        let base = "https://github.com/\(repo)/releases/latest/download"
        async let policy = get("\(base)/update-policy.json")
        async let signature = get("\(base)/update-policy.json.sig")
        return try checkUpdate(policy: try await policy, signature: try await signature, skippedVersion: skippedVersion)
    }

    private func get(_ address: String) async throws -> Data {
        guard let url = URL(string: address) else { throw URLError(.badURL) }
        var request = URLRequest(url: url)
        request.timeoutInterval = 15
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard status == 200 else {
            throw UpdateFetchError(message: "업데이트 정보를 받지 못했습니다 (HTTP \(status))")
        }
        // 데스크톱과 같은 상한
        guard data.count <= 64 * 1024 else {
            throw UpdateFetchError(message: "업데이트 정보가 너무 큽니다")
        }
        return data
    }
}

struct UpdateFetchError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

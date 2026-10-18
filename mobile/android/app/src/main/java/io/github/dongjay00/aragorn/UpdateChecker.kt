package io.github.dongjay00.aragorn

import io.github.dongjay00.aragorn.core.UpdateCheck
import io.github.dongjay00.aragorn.core.checkUpdate
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/**
 * GitHub Releases의 latest 릴리스에서 서명된 업데이트 정책(데스크톱과 같은 파일)을 받아 Rust에서 검증한다.
 * 네트워크 호출이라 백그라운드 스레드에서 부른다.
 */
class UpdateChecker(private val repo: String) {
    /** 이 빌드가 업데이트를 확인하는지. 릴리스 빌드만 저장소를 안다. */
    val enabled get() = repo.isNotEmpty()

    /** 새 APK를 받을 릴리스 페이지. */
    val releasePage get() = "https://github.com/$repo/releases/latest"

    /** 서명이 틀리거나 받을 수 없으면 예외를 던진다. */
    fun check(skippedVersion: String?): UpdateCheck {
        val base = "https://github.com/$repo/releases/latest/download"
        val policy = get("$base/update-policy.json")
        val signature = get("$base/update-policy.json.sig")
        return checkUpdate(policy, signature, skippedVersion)
    }

    private fun get(url: String): ByteArray {
        val connection = URL(url).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = TIMEOUT_MS
            connection.readTimeout = TIMEOUT_MS
            // GitHub은 다운로드 주소를 다른 호스트로 넘긴다. 둘 다 https다.
            connection.instanceFollowRedirects = true
            if (connection.responseCode != HttpURLConnection.HTTP_OK) {
                throw IOException("업데이트 정보를 받지 못했습니다 (HTTP ${connection.responseCode})")
            }
            return connection.inputStream.use { input ->
                val out = ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    out.write(buffer, 0, n)
                    if (out.size() > MAX_BYTES) throw IOException("업데이트 정보가 너무 큽니다")
                }
                out.toByteArray()
            }
        } finally {
            connection.disconnect()
        }
    }

    private companion object {
        const val TIMEOUT_MS = 15_000
        /** 데스크톱과 같은 상한 */
        const val MAX_BYTES = 64 * 1024
    }
}

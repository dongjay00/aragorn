package io.github.dongjay00.aragorn

import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.webkit.MimeTypeMap
import java.io.File
import java.io.FileNotFoundException

/**
 * 앱의 세이브 폴더(`files/saves`)를 시스템 파일 앱과 파일 선택기에 "Aragorn"으로 보인다.
 * 데스크톱 세이브(`<ROM 이름>.sav`, `.ss0`–`.ss9`)를 넣거나 꺼낼 수 있다 (iOS의 파일 앱 공유와 같다).
 *
 * 문서 ID는 `saves`(폴더)와 `saves/<파일 이름>`이다. 하위 폴더는 없다.
 */
class SavesProvider : DocumentsProvider() {
    private val savesDir: File
        get() = File(requireNotNull(context).filesDir, SAVES).apply { mkdirs() }

    override fun onCreate() = true

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val cursor = MatrixCursor(projection ?: DEFAULT_ROOT_PROJECTION)
        cursor.newRow().apply {
            add(Root.COLUMN_ROOT_ID, ROOT_ID)
            add(Root.COLUMN_DOCUMENT_ID, SAVES)
            add(Root.COLUMN_TITLE, requireNotNull(context).getString(R.string.app_name))
            add(Root.COLUMN_SUMMARY, "게임 세이브")
            add(Root.COLUMN_ICON, R.mipmap.ic_launcher)
            add(
                Root.COLUMN_FLAGS,
                Root.FLAG_SUPPORTS_CREATE or Root.FLAG_LOCAL_ONLY or Root.FLAG_SUPPORTS_IS_CHILD,
            )
            add(Root.COLUMN_AVAILABLE_BYTES, savesDir.freeSpace)
        }
        return cursor
    }

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        MatrixCursor(projection ?: DEFAULT_DOCUMENT_PROJECTION).also { addRow(it, documentId) }

    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        if (parentDocumentId != SAVES) throw FileNotFoundException(parentDocumentId)
        val cursor = MatrixCursor(projection ?: DEFAULT_DOCUMENT_PROJECTION)
        savesDir.listFiles()
            ?.filter { it.isFile && !it.name.endsWith(".tmp") }
            ?.sortedBy { it.name }
            ?.forEach { addRow(cursor, "$SAVES/${it.name}") }
        cursor.setNotificationUri(requireNotNull(context).contentResolver, childrenUri())
        return cursor
    }

    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        val file = fileFor(documentId)
        val writing = mode.contains('w')
        if (writing) RunningGame.beforeExternalWrite(file.name)
        val accessMode = ParcelFileDescriptor.parseMode(mode)
        if (!writing) return ParcelFileDescriptor.open(file, accessMode)
        // 다 쓰고 닫을 때 목록을 갱신한다.
        val handler = android.os.Handler(android.os.Looper.getMainLooper())
        return ParcelFileDescriptor.open(file, accessMode, handler) { notifyChanged() }
    }

    /**
     * 파일 앱에서 세이브를 붙여 넣으면 불린다. 같은 이름이 있으면 덮어쓰기 전에 `<이름>.bak`으로 남긴다.
     * 데스크톱 세이브는 이름이 정확해야(ROM 이름 + 확장자) 이어서 할 수 있어 이름을 바꾸지 않는다.
     */
    override fun createDocument(parentDocumentId: String, mimeType: String, displayName: String): String {
        if (parentDocumentId != SAVES) throw FileNotFoundException(parentDocumentId)
        if (mimeType == Document.MIME_TYPE_DIR) throw UnsupportedOperationException("폴더는 만들 수 없습니다")
        val name = safeName(displayName)
        val file = File(savesDir, name)
        RunningGame.beforeExternalWrite(name)
        if (file.exists()) {
            file.copyTo(File(savesDir, "$name.bak"), overwrite = true)
            file.delete()
        }
        file.createNewFile()
        notifyChanged()
        return "$SAVES/$name"
    }

    override fun deleteDocument(documentId: String) {
        val file = fileFor(documentId)
        RunningGame.beforeExternalWrite(file.name)
        if (!file.delete()) throw FileNotFoundException(documentId)
        notifyChanged()
    }

    override fun renameDocument(documentId: String, displayName: String): String {
        val file = fileFor(documentId)
        val name = safeName(displayName)
        RunningGame.beforeExternalWrite(file.name)
        RunningGame.beforeExternalWrite(name)
        if (!file.renameTo(File(savesDir, name))) throw FileNotFoundException(documentId)
        notifyChanged()
        return "$SAVES/$name"
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String) =
        parentDocumentId == SAVES && documentId.startsWith("$SAVES/")

    override fun getDocumentType(documentId: String): String =
        if (documentId == SAVES) Document.MIME_TYPE_DIR else mimeTypeOf(documentId)

    private fun addRow(cursor: MatrixCursor, documentId: String) {
        cursor.newRow().apply {
            if (documentId == SAVES) {
                add(Document.COLUMN_DOCUMENT_ID, SAVES)
                add(Document.COLUMN_DISPLAY_NAME, "saves")
                add(Document.COLUMN_MIME_TYPE, Document.MIME_TYPE_DIR)
                add(Document.COLUMN_FLAGS, Document.FLAG_DIR_SUPPORTS_CREATE)
                add(Document.COLUMN_LAST_MODIFIED, savesDir.lastModified())
            } else {
                val file = fileFor(documentId)
                if (!file.isFile) throw FileNotFoundException(documentId)
                add(Document.COLUMN_DOCUMENT_ID, documentId)
                add(Document.COLUMN_DISPLAY_NAME, file.name)
                add(Document.COLUMN_MIME_TYPE, mimeTypeOf(file.name))
                add(Document.COLUMN_SIZE, file.length())
                add(Document.COLUMN_LAST_MODIFIED, file.lastModified())
                add(
                    Document.COLUMN_FLAGS,
                    Document.FLAG_SUPPORTS_WRITE or Document.FLAG_SUPPORTS_DELETE or
                        Document.FLAG_SUPPORTS_RENAME,
                )
            }
        }
    }

    /** 문서 ID를 파일로 바꾼다. 세이브 폴더 밖을 가리키는 ID는 거부한다. */
    private fun fileFor(documentId: String): File {
        val name = documentId.removePrefix("$SAVES/")
        if (name == documentId || name != safeName(name)) throw FileNotFoundException(documentId)
        return File(savesDir, name)
    }

    private fun notifyChanged() {
        requireNotNull(context).contentResolver.notifyChange(childrenUri(), null)
    }

    private fun childrenUri() =
        DocumentsContract.buildChildDocumentsUri(authority(), SAVES)

    private fun authority() = "${requireNotNull(context).packageName}.saves"

    companion object {
        private const val ROOT_ID = "aragorn"
        private const val SAVES = "saves"

        private val DEFAULT_ROOT_PROJECTION = arrayOf(
            Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_TITLE, Root.COLUMN_SUMMARY,
            Root.COLUMN_ICON, Root.COLUMN_FLAGS, Root.COLUMN_AVAILABLE_BYTES,
        )
        private val DEFAULT_DOCUMENT_PROJECTION = arrayOf(
            Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE,
            Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED, Document.COLUMN_FLAGS,
        )

        /** 경로 구분자와 `.`/`..`을 막는다. 앱이 세이브 이름을 만드는 규칙(`/` → `_`)과 같다. */
        fun safeName(name: String): String {
            val cleaned = name.replace('/', '_').replace('\u0000', '_').trim()
            if (cleaned.isEmpty() || cleaned == "." || cleaned == "..") throw FileNotFoundException(name)
            return cleaned
        }

        private fun mimeTypeOf(name: String): String =
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', ""))
                ?: "application/octet-stream"
    }
}

/**
 * 지금 실행 중인 게임의 세이브 이름. 파일 앱에서 그 게임의 세이브를 바꾸려 하면, 바꾸기 전에 게임을 닫는다.
 * 닫지 않으면 게임이 나중에 자기 진행으로 가져온 세이브를 덮어쓴다.
 */
object RunningGame {
    /** 세이브 파일 이름의 앞부분 (`Pokemon Crystal.gbc` → `Pokemon Crystal`). */
    @Volatile private var stem: String? = null
    @Volatile private var close: (() -> Unit)? = null

    fun set(saveFileName: String?, onClose: (() -> Unit)?) {
        stem = saveFileName?.substringBeforeLast('.', saveFileName)
        close = onClose
    }

    /** 바꾸려는 파일이 실행 중인 게임의 세이브면, 세이브를 저장하고 게임을 닫는다 (호출한 스레드에서 끝낸다). */
    fun beforeExternalWrite(fileName: String) {
        val current = stem ?: return
        if (fileName == current || fileName.startsWith("$current.")) {
            close?.invoke()
            stem = null
            close = null
        }
    }
}

package io.github.dongjay00.aragorn

import android.content.Context

/** Rust 라이브러리에 앱 Context를 넘긴다. 오디오 출력(cpal)이 Java `AudioManager`를 부를 때 쓴다. */
object AndroidContext {
    init {
        System.loadLibrary("aragorn_mobile")
    }

    /** 에뮬레이터를 만들기 전에 한 번 부른다. 여러 번 불러도 된다. */
    external fun init(context: Context)
}

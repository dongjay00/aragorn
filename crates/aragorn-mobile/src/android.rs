//! cpal의 AAudio 출력은 버퍼 크기를 정하려고 Java `AudioManager`를 부른다. 그러려면 ndk-context에
//! JavaVM과 앱 Context가 들어 있어야 한다. 앱이 시작할 때 Kotlin `AndroidContext.init`이 이 함수로 넘긴다.

use jni_sys::{JNI_OK, JNIEnv, JavaVM, jclass, jobject};
use std::{ptr, sync::Once};

/// Kotlin: `io.github.dongjay00.aragorn.AndroidContext.init(context: Context)`
///
/// # Safety
/// JVM이 JNI 규약대로 부른다. `context`는 살아 있는 `android.content.Context`여야 한다.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_io_github_dongjay00_aragorn_AndroidContext_init(
    env: *mut JNIEnv,
    _class: jclass,
    context: jobject,
) {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        let functions = &**env;
        let mut vm: *mut JavaVM = ptr::null_mut();
        if (functions.v1_1.GetJavaVM)(env, &mut vm) != JNI_OK {
            log::error!("JavaVM을 얻지 못해 오디오를 쓸 수 없습니다");
            return;
        }
        // 앱이 끝날 때까지 쓰므로 전역 참조를 풀지 않는다.
        let context = (functions.v1_1.NewGlobalRef)(env, context);
        ndk_context::initialize_android_context(vm.cast(), context.cast());
    });
}

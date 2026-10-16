//! Windows 실행 파일에 앱 아이콘을 넣는다. 아이콘은 `scripts/gen-icons.py`로 만든다.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=assets/icon/aragorn.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon/aragorn.ico")
            .compile()
            .expect("Windows 리소스(아이콘)를 컴파일하지 못했습니다");
    }
}

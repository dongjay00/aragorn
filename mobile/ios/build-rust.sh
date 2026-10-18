#!/bin/sh
# Xcode 빌드의 첫 단계: Rust 라이브러리와 Swift 바인딩을 만든다 (cargo xtask ios).
set -eu

case "$CONFIGURATION" in
    Release) PROFILE="--release" ;;
    *) PROFILE="" ;;
esac

# Xcode가 넣는 SDKROOT 같은 환경 변수가 있으면 호스트(macOS)용 빌드 스크립트(build.rs)까지
# iOS SDK로 빌드하려다 실패한다. 필요한 것만 남기고 비운다.
# Xcode를 Dock에서 열면 PATH에 ~/.cargo/bin이 없어 직접 넣는다.
exec env -i \
    HOME="$HOME" \
    PATH="$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    IPHONEOS_DEPLOYMENT_TARGET="$IPHONEOS_DEPLOYMENT_TARGET" \
    cargo xtask ios --platform="$PLATFORM_NAME" $PROFILE

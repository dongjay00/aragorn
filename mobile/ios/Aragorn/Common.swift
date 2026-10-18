import SwiftUI
import UIKit

extension Image {
    /// Rust가 준 RGBA 이미지. 투명한 픽셀(스프라이트 색 0)은 투명하게 그린다.
    var cgImage: CGImage? {
        makeCGImage(width: Int(width), height: Int(height), rgba: rgba, opaque: false)
    }
}

/// RGBA 바이트(R, G, B, A 순서)를 CGImage로 바꾼다.
func makeCGImage(width: Int, height: Int, rgba: Data, opaque: Bool) -> CGImage? {
    guard rgba.count == width * height * 4, let provider = CGDataProvider(data: rgba as CFData) else {
        return nil
    }
    let alpha: CGImageAlphaInfo = opaque ? .noneSkipLast : .premultipliedLast
    return CGImage(
        width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGBitmapInfo(rawValue: alpha.rawValue),
        provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
    )
}

/// 0xRRGGBBAA → SwiftUI 색.
func rgbaColor(_ rgba: UInt32) -> Color {
    Color(
        .sRGB,
        red: Double((rgba >> 24) & 0xFF) / 255,
        green: Double((rgba >> 16) & 0xFF) / 255,
        blue: Double((rgba >> 8) & 0xFF) / 255,
        opacity: Double(rgba & 0xFF) / 255
    )
}

/// 픽셀이 뭉개지지 않게 가장 가까운 이웃으로 늘려 그린다 (VRAM, 스프라이트, 썸네일).
struct PixelImage: View {
    let image: CGImage

    var body: some View {
        SwiftUI.Image(decorative: image, scale: 1)
            .resizable()
            .interpolation(.none)
            .aspectRatio(CGFloat(image.width) / CGFloat(image.height), contentMode: .fit)
    }
}

let mono = Font.system(.caption, design: .monospaced)

/// 게임 화면. 디스플레이 갱신마다 프레임 번호를 보고, 바뀌었을 때만 픽셀을 가져와 그린다.
/// `onFrame`은 화면 갱신마다 부른다 (안내 문구, 실행 상태 갱신).
struct LcdView: UIViewRepresentable {
    let emulator: Emulator
    var onFrame: () -> Void = {}

    func makeUIView(context: Context) -> LcdLayerView {
        let view = LcdLayerView()
        view.start(emulator: emulator, onFrame: onFrame)
        return view
    }

    func updateUIView(_ view: LcdLayerView, context: Context) {
        view.start(emulator: emulator, onFrame: onFrame)
    }

    static func dismantleUIView(_ view: LcdLayerView, coordinator: ()) {
        view.stop()
    }
}

final class LcdLayerView: UIView {
    private var emulator: Emulator?
    private var onFrame: () -> Void = {}
    private var link: CADisplayLink?
    private var shown = UInt64.max
    private let size = screenSize()

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        layer.magnificationFilter = .nearest
        layer.contentsGravity = .resize
    }

    required init?(coder: NSCoder) { fatalError("코드에서만 만든다") }

    func start(emulator: Emulator, onFrame: @escaping () -> Void) {
        self.onFrame = onFrame
        if self.emulator !== emulator {
            self.emulator = emulator
            shown = .max
        }
        if link == nil {
            let link = CADisplayLink(target: self, selector: #selector(tick))
            link.add(to: .main, forMode: .common)
            self.link = link
        }
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func tick() {
        guard let emulator else { return }
        let number = emulator.frameNumber()
        if number != shown {
            shown = number
            layer.contents = makeCGImage(
                width: Int(size.width), height: Int(size.height), rgba: emulator.frame(), opaque: true
            )
        }
        onFrame()
    }
}

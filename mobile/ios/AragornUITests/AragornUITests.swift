import XCTest

/// 시뮬레이터에서 앱을 띄워 터치로 조작해 본다. 디버그 빌드의 `-rom` 실행 인자로 테스트 ROM을 연다.
final class AragornUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-rom", try makeRom().path]
        app.launch()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "ARAGORN TEST 실행 중", timeout: 10))
    }

    /// 제자리 점프(`JR -2`, $0100)만 하는 ROM.
    private func makeRom() throws -> URL {
        var rom = [UInt8](repeating: 0, count: 0x8000)
        rom[0x0100] = 0x18
        rom[0x0101] = 0xFE
        let title = Array("ARAGORN TEST".utf8)
        rom.replaceSubrange(0x0134..<0x0134 + title.count, with: title)
        let url = FileManager.default.temporaryDirectory.appending(path: "uitest.gb")
        try Data(rom).write(to: url)
        return url
    }

    private var notice: XCUIElement { app.staticTexts["status.notice"] }

    private func element(_ identifier: String) -> XCUIElement {
        app.descendants(matching: .any)[identifier]
    }

    func testPauseButtonStopsTheGame() {
        app.buttons["일시정지"].tap()
        XCTAssertTrue(app.staticTexts["status.speed"].wait(for: \.label, toEqual: "일시정지", timeout: 5))
        app.buttons["계속"].tap()
        XCTAssertTrue(app.staticTexts["status.speed"].wait(for: \.label, toEqual: "1배", timeout: 5))
    }

    func testStateSavedFromSlotSheetShowsUp() {
        app.buttons["스테이트"].tap()
        app.buttons["저장"].firstMatch.tap()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "슬롯 1에 저장했습니다", timeout: 5))
        app.buttons["스테이트"].tap()
        XCTAssertTrue(app.staticTexts["슬롯 1 · 방금"].waitForExistence(timeout: 5))
        app.buttons["불러오기"].firstMatch.tap()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "슬롯 1을(를) 불러왔습니다", timeout: 5))
    }

    func testPadButtonsAcceptTouches() {
        // 누르고 떼는 동안 앱이 멈추거나 꺼지지 않는지 본다. 미끄러뜨리기도 해 본다.
        element("pad.A").press(forDuration: 0.3)
        element("dpad.◀").press(forDuration: 0.3, thenDragTo: element("dpad.▲"))
        element("pad.▶▶ 배속").press(forDuration: 0.5)
        XCTAssertTrue(app.staticTexts["status.speed"].wait(for: \.label, toEqual: "1배", timeout: 5))
    }

    func testSettingsWaitsForAKey() {
        app.buttons["메뉴"].tap()
        app.buttons["설정"].tap()
        app.buttons["X"].tap()
        XCTAssertTrue(app.buttons["입력 대기…"].waitForExistence(timeout: 5))
        app.buttons["취소"].tap()
        XCTAssertTrue(app.buttons["X"].waitForExistence(timeout: 5))
        app.buttons["돌아가기"].tap()
        XCTAssertTrue(app.buttons["스테이트"].waitForExistence(timeout: 5))
    }

    func testBreakpointStopsAtThePc() {
        app.buttons["메뉴"].tap()
        app.buttons["디버거"].tap()
        let row = element("disasm.0100")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "브레이크포인트 $0100에서 멈췄습니다", timeout: 5))
        XCTAssertTrue(app.buttons["계속"].exists)
    }

    func testLandscapePutsPadsBesideTheScreen() {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        let left = element("dpad.◀")
        let a = element("pad.A")
        XCTAssertTrue(left.waitForExistence(timeout: 5))
        XCTAssertTrue(left.isHittable)
        XCTAssertTrue(a.isHittable)
        // 방향키는 왼쪽, A는 오른쪽 끝에 있다.
        XCTAssertLessThan(left.frame.maxX, app.frame.width / 3)
        XCTAssertGreaterThan(a.frame.minX, app.frame.width * 2 / 3)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }
}

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

    /// 툴바 메뉴(⋯)를 열고 항목을 누른다. 메뉴가 다 열릴 때까지 기다린다.
    private func openMenu(_ item: String) {
        app.buttons["메뉴"].tap()
        let button = app.buttons[item]
        XCTAssertTrue(button.waitForExistence(timeout: 5), "메뉴에 \(item)이(가) 없습니다")
        button.tap()
    }

    /// 목록은 화면에 보이는 줄만 만든다. 보일 때까지 아래로 넘긴다.
    private func scrollTo(_ element: XCUIElement) -> XCUIElement {
        for _ in 0..<6 where !(element.exists && element.isHittable) {
            app.swipeUp()
        }
        return element
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
        openMenu("설정")
        app.buttons["X"].tap()
        XCTAssertTrue(app.buttons["입력 대기…"].waitForExistence(timeout: 5))
        app.buttons["취소"].tap()
        XCTAssertTrue(app.buttons["X"].waitForExistence(timeout: 5))
        app.buttons["돌아가기"].tap()
        XCTAssertTrue(app.buttons["스테이트"].waitForExistence(timeout: 5))
    }

    func testBreakpointStopsAtThePc() {
        openMenu("디버거")
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

    func testHapticsSettingIsSaved() {
        openMenu("설정")
        let toggle = scrollTo(app.switches["버튼 햅틱"])
        XCTAssertTrue(toggle.exists)
        XCTAssertEqual(toggle.value as? String, "1", "기본은 켜짐")
        toggle.switches.firstMatch.tap()
        XCTAssertEqual(toggle.value as? String, "0")

        // 다시 켜도 꺼진 채로 남는다.
        app.terminate()
        app.launch()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "ARAGORN TEST 실행 중", timeout: 10))
        openMenu("설정")
        let reopened = scrollTo(app.switches["버튼 햅틱"])
        XCTAssertEqual(reopened.value as? String, "0")
        // 다음 테스트를 위해 되돌린다.
        reopened.switches.firstMatch.tap()
        XCTAssertEqual(reopened.value as? String, "1")
    }

    func testCheatsAreCheckedAddedAndToggled() {
        openMenu("치트")
        let code = app.textFields["cheat.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 5))
        // 앞서 실패한 실행이 남긴 치트(세이브 옆 .cht)를 지운다.
        for _ in 0..<5 where app.switches["cheat.0"].exists {
            deleteFirstCheat()
        }
        code.tap()
        code.typeText("nope")
        app.buttons["더하기"].tap()
        XCTAssertTrue(app.staticTexts["cheat.notice"].label.contains("코드가 아닙니다"))
        code.tap()
        code.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 4) + "010138cd")
        app.textFields["cheat.name"].tap()
        app.textFields["cheat.name"].typeText("체력")
        app.buttons["더하기"].tap()
        let toggle = app.switches["cheat.0"]
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? String, "1", "더하면 켜진다")
        toggle.switches.firstMatch.tap()
        XCTAssertEqual(toggle.value as? String, "0")
        // 다음 테스트를 위해 지운다.
        deleteFirstCheat()
        XCTAssertTrue(app.staticTexts["아직 치트가 없습니다."].waitForExistence(timeout: 5))
    }

    private func deleteFirstCheat() {
        app.switches["cheat.0"].swipeLeft()
        // 지우기 버튼 이름은 시뮬레이터 언어를 따른다.
        app.buttons.matching(NSPredicate(format: "label IN %@", ["Delete", "삭제"])).firstMatch.tap()
    }

    func testRecentGameReopensWithoutPicker() {
        app.terminate()
        app.launchArguments = []
        app.launch()
        let recent = app.buttons["recent.ARAGORN TEST"].firstMatch
        XCTAssertTrue(recent.waitForExistence(timeout: 10))
        recent.tap()
        XCTAssertTrue(notice.wait(for: \.label, toEqual: "ARAGORN TEST 실행 중", timeout: 10))
    }

    func testRewindButtonIsBesideFastForward() {
        let rewind = element("pad.◀◀ 되감기")
        XCTAssertTrue(rewind.waitForExistence(timeout: 5))
        rewind.press(forDuration: 0.5)
        XCTAssertTrue(app.staticTexts["status.speed"].wait(for: \.label, toEqual: "1배", timeout: 5))
    }

    func testPadSizeSettingMakesButtonsBigger() {
        let before = element("pad.A").frame.width
        openMenu("설정")
        let slider = scrollTo(app.sliders.firstMatch)
        slider.adjust(toNormalizedSliderPosition: 1.0)
        app.buttons["돌아가기"].tap()
        let after = element("pad.A")
        XCTAssertTrue(after.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(after.frame.width, before + 4)
        // 다음 테스트를 위해 되돌린다.
        openMenu("설정")
        scrollTo(app.buttons["가상 패드를 기본값으로"]).tap()
        app.buttons["돌아가기"].tap()
    }
}

import XCTest

final class CheetahUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testSendMessage() throws {
        let app = XCUIApplication()
        app.launch()

        let field = app.textFields["cheetah-composer"]
        XCTAssertTrue(field.waitForExistence(timeout: 20), "composer should be visible")
        field.tap()
        field.typeText("hello from ios")

        let send = app.buttons["cheetah-send"]
        XCTAssertTrue(send.waitForExistence(timeout: 5), "send button should be visible")
        send.tap()

        let message = app.staticTexts["hello from ios"]
        XCTAssertTrue(message.waitForExistence(timeout: 10), "sent message should render in the list")
    }

    func testDraftSurvivesBackgrounding() throws {
        let app = XCUIApplication()
        app.launch()

        let field = app.textFields["cheetah-composer"]
        XCTAssertTrue(field.waitForExistence(timeout: 20), "composer should be visible")
        field.tap()
        field.typeText("draft survives")

        XCUIDevice.shared.press(.home)
        sleep(2)
        app.activate()

        let restored = app.textFields["cheetah-composer"]
        XCTAssertTrue(restored.waitForExistence(timeout: 15), "composer should return")
        XCTAssertEqual(restored.value as? String, "draft survives", "draft should survive backgrounding")
    }

    func testScrollPerformance() throws {
        let app = XCUIApplication()
        app.launch()

        let list = app.scrollViews.firstMatch
        XCTAssertTrue(list.waitForExistence(timeout: 20), "message list should be visible")

        measure(metrics: [XCTOSSignpostMetric.scrollingAndDecelerationMetric]) {
            list.swipeUp(velocity: .fast)
        }
    }
}

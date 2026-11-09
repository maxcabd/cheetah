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

        sleep(2)
        print("FIELD VALUE AFTER SEND: \(String(describing: field.value))")
        let labels = app.staticTexts.allElementsBoundByIndex.map { $0.label }
        print("STATIC LABELS: \(labels)")
        print("APP HIERARCHY:\n\(app.debugDescription)")

        let message = app.staticTexts["hello from ios"]
        XCTAssertTrue(message.waitForExistence(timeout: 10), "sent message should render in the list")
    }
}

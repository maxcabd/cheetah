import XCTest

/// What a person feels, measured from outside the app: startup, views
/// changing, and scrolling. `testNative...` does the same thing in Settings,
/// Apple's own UIKit app, on the same simulator: the gap to it is the goal.
/// Simulator numbers compare with each other, not with a phone.
final class CheetahPerformanceTests: XCTestCase {
    private var options: XCTMeasureOptions {
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        return options
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    private func nightshade(_ argument: String = "--nightshade-demo") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [argument]
        return app
    }

    private var settings: XCUIApplication {
        XCUIApplication(bundleIdentifier: "com.apple.Preferences")
    }

    // MARK: Startup

    /// Process start to the first frame, as iOS reports it.
    func testStartupToFirstFrame() throws {
        let app = nightshade()
        measure(metrics: [XCTApplicationLaunchMetric()], options: options) {
            app.launch()
        }
    }

    /// Launch to a result card on screen.
    func testStartupToContent() throws {
        startup(nightshade()) { $0.staticTexts["Java Haven"] }
    }

    func testNativeStartupToContent() throws {
        startup(settings) { $0.cells.firstMatch }
    }

    private func startup(_ app: XCUIApplication, content: (XCUIApplication) -> XCUIElement) {
        let options = self.options
        options.invocationOptions = [.manuallyStart, .manuallyStop]
        measure(metrics: [XCTClockMetric()], options: options) {
            app.terminate()
            startMeasuring()
            app.launch()
            XCTAssertTrue(content(app).waitForExistence(timeout: 20), "content should appear")
            stopMeasuring()
        }
    }

    // MARK: Views changing

    /// A tap on a suggestion to its result card: a new screen of views.
    func testSearchToResults() throws {
        let app = nightshade("--nightshade")
        app.launch()
        let suggestion = app.descendants(matching: .any)["Cafes within 12 min walk"]
        let result = app.staticTexts["Java Haven"]
        change(from: suggestion, to: result) {
            app.buttons["Clear"].tap()
            XCTAssertTrue(suggestion.waitForExistence(timeout: 10), "results should clear")
        }
        XCTAssertFalse(result.exists)
    }

    /// A tap on a settings row to its screen.
    func testNativeRowToScreen() throws {
        let app = settings
        app.launch()
        let row = app.staticTexts["General"].firstMatch
        let back = app.navigationBars.buttons.firstMatch
        let screen = app.staticTexts["About"].firstMatch
        change(from: row, to: screen) {
            back.tap()
            XCTAssertTrue(row.waitForExistence(timeout: 10), "should go back")
        }
    }

    private func change(from: XCUIElement, to: XCUIElement, reset: () -> Void) {
        XCTAssertTrue(from.waitForExistence(timeout: 20), "start should be visible")
        let options = self.options
        options.invocationOptions = [.manuallyStart, .manuallyStop]
        measure(metrics: [XCTClockMetric()], options: options) {
            startMeasuring()
            from.tap()
            XCTAssertTrue(to.waitForExistence(timeout: 10), "the new view should appear")
            stopMeasuring()
            reset()
        }
    }

    // MARK: Scrolling

    /// Frames and hitches while the result cards page sideways.
    func testCarouselScroll() throws {
        let app = nightshade()
        app.launch()
        let card = app.descendants(matching: .any)["card-java"]
        XCTAssertTrue(card.waitForExistence(timeout: 20), "result cards should be visible")
        let cards = app.scrollViews.containing(.any, identifier: "card-java").firstMatch
        XCTAssertTrue(cards.exists, "the carousel should be a scroll view")
        measure(metrics: [XCTOSSignpostMetric.scrollingAndDecelerationMetric], options: options) {
            cards.swipeLeft(velocity: .fast)
            cards.swipeRight(velocity: .fast)
        }
    }
}

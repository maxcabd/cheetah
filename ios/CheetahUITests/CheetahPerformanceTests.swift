import XCTest

/// What a person feels, measured from outside the app: startup, views
/// changing, and scrolling. Each `testCheetah...` has a `testNative...` that
/// does the same thing in the same screens written in UIKit (ios/NativeTwin),
/// on the same device: the gap is what Cheetah costs. Simulator numbers
/// compare with each other, not with a phone.
///
/// The native ones are skipped when NativeTwin isn't installed.
final class CheetahPerformanceTests: XCTestCase {
    private static let twinID = "com.example.cheetah.twin"
    private var launched: XCUIApplication?

    private var options: XCTMeasureOptions {
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        options.invocationOptions = [.manuallyStart, .manuallyStop]
        return options
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    override func tearDown() {
        // Ended, not left in the background, where it would save its state
        // and change what the next test starts from.
        launched?.terminate()
        launched = nil
    }

    private func cheetah(_ argument: String = "--nightshade-demo") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [argument]
        launched = app
        return app
    }

    private func twin(_ arguments: [String] = []) throws -> XCUIApplication {
        guard ProcessInfo.processInfo.environment["NATIVE_TWIN"] == "1" else {
            throw XCTSkip("NativeTwin isn't installed (set TEST_RUNNER_NATIVE_TWIN=1 when it is)")
        }
        let app = XCUIApplication(bundleIdentifier: Self.twinID)
        app.launchArguments = arguments
        launched = app
        return app
    }

    // MARK: Startup: launch to a result card on screen

    func testCheetahStartup() throws {
        startup(cheetah())
    }

    func testNativeStartup() throws {
        startup(try twin())
    }

    private func startup(_ app: XCUIApplication) {
        // The first launch after an install is slower for both: not counted.
        app.launch()
        XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 30), "a card should appear")
        measure(metrics: [XCTClockMetric()], options: options) {
            app.terminate()
            startMeasuring()
            app.launch()
            XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 20), "a card should appear")
            stopMeasuring()
        }
    }

    // MARK: Views changing

    /// A tap on a suggestion to its result card.
    func testCheetahResults() throws {
        results(cheetah("--nightshade"))
    }

    func testNativeResults() throws {
        results(try twin(["--suggestions"]))
    }

    private func results(_ app: XCUIApplication) {
        app.launch()
        let suggestion = app.descendants(matching: .any)["Cafes within 12 min walk"].firstMatch
        let result = app.staticTexts["Java Haven"].firstMatch
        let clear = app.buttons["Clear"].firstMatch
        XCTAssertTrue(suggestion.waitForExistence(timeout: 20), "suggestions should be visible")
        measure(metrics: [XCTClockMetric(), XCTCPUMetric(application: app)], options: options) {
            startMeasuring()
            suggestion.tap()
            XCTAssertTrue(result.waitForExistence(timeout: 10), "a card should appear")
            stopMeasuring()
            clear.tap()
            XCTAssertTrue(suggestion.waitForExistence(timeout: 10), "results should clear")
        }
    }

    /// A tap on a card to its page.
    func testCheetahPage() throws {
        page(cheetah())
    }

    func testNativePage() throws {
        page(try twin())
    }

    private func page(_ app: XCUIApplication) {
        app.launch()
        let card = app.descendants(matching: .any)["card-java"].firstMatch
        let close = app.buttons["Close"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "result cards should be visible")
        measure(metrics: [XCTClockMetric(), XCTCPUMetric(application: app)], options: options) {
            startMeasuring()
            // Below the photo, which takes touches of its own.
            card.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.62)).tap()
            XCTAssertTrue(close.waitForExistence(timeout: 10), "the page should open")
            stopMeasuring()
            close.tap()
            let closed = XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "exists == false"), object: close
            )
            XCTAssertEqual(XCTWaiter().wait(for: [closed], timeout: 10), .completed, "the page should close")
        }
    }

    // MARK: Scrolling: paging the result cards

    func testCheetahScroll() throws {
        scroll(cheetah())
    }

    func testNativeScroll() throws {
        scroll(try twin())
    }

    private func scroll(_ app: XCUIApplication) {
        app.launch()
        let card = app.descendants(matching: .any)["card-java"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "result cards should be visible")
        let cards = app.scrollViews.containing(.any, identifier: "card-java").firstMatch
        XCTAssertTrue(cards.exists, "the carousel should be a scroll view")
        let metrics: [XCTMetric] = [
            XCTOSSignpostMetric.scrollingAndDecelerationMetric,
            XCTCPUMetric(application: app),
        ]
        measure(metrics: metrics, options: options) {
            startMeasuring()
            // Along the text, below the photo's own pages.
            let right = cards.coordinate(withNormalizedOffset: CGVector(dx: 0.85, dy: 0.68))
            let left = cards.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.68))
            right.press(forDuration: 0.01, thenDragTo: left, withVelocity: .fast, thenHoldForDuration: 0)
            left.press(forDuration: 0.01, thenDragTo: right, withVelocity: .fast, thenHoldForDuration: 0)
            stopMeasuring()
        }
    }
}

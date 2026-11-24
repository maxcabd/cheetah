import XCTest

/// What a person feels, measured from outside the app: startup, views
/// changing, and scrolling. Each `testCheetah...` has a `testNative...` that
/// does the same thing in the same screens written in UIKit (ios/NativeTwin),
/// on the same device: the gap is what Cheetah costs. Simulator numbers
/// compare with each other, not with a phone.
///
/// `testReact...` is the same product in React Native (the app Nightshade
/// started as, in its benchmark mode), and `testTuned...` that app with a
/// native sheet and native paging instead of its own JS ones.
///
/// Each is skipped when its app isn't installed.
///
/// Except for startup, only the app's CPU time is measured, and the test stays
/// out of it meanwhile. Looking for an element makes the app describe its
/// whole accessibility tree, which costs far more than what is being measured
/// (over 99% of Cheetah's samples when opening a page) and grows with the
/// number of views. So taps go to a position on the home screen's app, not to
/// an element, and nothing is looked up until the measurement stops.
final class CheetahPerformanceTests: XCTestCase {
    private static let twinID = "com.example.cheetah.twin"
    private static let reactID = "com.nightshade.nightshadeapp"
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

    private func react(tuned: Bool = false, suggestions: Bool = false) throws -> XCUIApplication {
        guard ProcessInfo.processInfo.environment["REACT_TWIN"] == "1" else {
            throw XCTSkip("the React Native app isn't installed (set TEST_RUNNER_REACT_TWIN=1 when it is)")
        }
        let app = XCUIApplication(bundleIdentifier: Self.reactID)
        if tuned {
            app.launchArguments += ["-benchVariant", "native"]
        }
        if suggestions {
            app.launchArguments += ["-benchScreen", "suggestions"]
        }
        launched = app
        return app
    }

    private let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

    /// Long enough for every version's transition to finish.
    private let settle: TimeInterval = 2.5

    /// A position on screen, for tapping without asking the app anything.
    private func spot(_ point: CGPoint, in app: XCUIApplication) -> XCUICoordinate {
        springboard.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: point.x, dy: point.y))
    }

    private func center(_ element: XCUIElement, _ dx: CGFloat = 0.5, _ dy: CGFloat = 0.5) -> CGPoint {
        let frame = element.frame
        return CGPoint(x: frame.minX + frame.width * dx, y: frame.minY + frame.height * dy)
    }

    /// With PROFILE=1 (ios-profile.yml): repeat the steps for a while instead
    /// of measuring, so a sampler has something to see.
    private func profile(_ steps: () -> Void) -> Bool {
        guard ProcessInfo.processInfo.environment["PROFILE"] == "1" else { return false }
        FileManager.default.createFile(atPath: "/tmp/cheetah-profile-start", contents: nil)
        let end = Date().addingTimeInterval(45)
        while Date() < end {
            steps()
        }
        return true
    }

    // MARK: Startup: launch to a result card on screen

    func testCheetahStartup() throws {
        startup(cheetah())
    }

    func testNativeStartup() throws {
        startup(try twin())
    }

    func testReactStartup() throws {
        startup(try react())
    }

    func testTunedStartup() throws {
        startup(try react(tuned: true))
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

    func testReactResults() throws {
        results(try react(suggestions: true))
    }

    func testTunedResults() throws {
        results(try react(tuned: true, suggestions: true))
    }

    private func results(_ app: XCUIApplication) {
        app.launch()
        let suggestion = app.descendants(matching: .any)["Cafes within 12 min walk"].firstMatch
        XCTAssertTrue(suggestion.waitForExistence(timeout: 20), "suggestions should be visible")
        sleep(2)
        let show = spot(center(suggestion), in: app)
        show.tap()
        let clear = app.buttons["Clear"].firstMatch
        XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 10), "a card should appear")
        XCTAssertTrue(clear.waitForExistence(timeout: 10), "results should have a clear button")
        sleep(2)
        let reset = spot(center(clear), in: app)
        reset.tap()
        XCTAssertTrue(suggestion.waitForExistence(timeout: 10), "results should clear")
        sleep(2)

        if profile({
            show.tap()
            Thread.sleep(forTimeInterval: 1)
            reset.tap()
            Thread.sleep(forTimeInterval: 1)
        }) { return }
        measure(metrics: [XCTCPUMetric(application: app)], options: options) {
            startMeasuring()
            show.tap()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
            reset.tap()
            Thread.sleep(forTimeInterval: settle)
        }
        show.tap()
        XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 10), "the taps should still work")
    }

    /// A tap on a card to its page.
    func testCheetahPage() throws {
        page(cheetah())
    }

    func testNativePage() throws {
        page(try twin())
    }

    func testReactPage() throws {
        page(try react())
    }

    func testTunedPage() throws {
        page(try react(tuned: true))
    }

    private func page(_ app: XCUIApplication) {
        app.launch()
        let card = app.descendants(matching: .any)["card-java"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "result cards should be visible")
        sleep(2)
        // Below the photo, which takes touches of its own.
        let open = spot(center(card, 0.5, 0.62), in: app)
        open.tap()
        let close = app.buttons["Close"].firstMatch
        XCTAssertTrue(close.waitForExistence(timeout: 10), "the page should open")
        sleep(2)
        let back = spot(center(close), in: app)
        back.tap()
        sleep(3)

        if profile({
            open.tap()
            Thread.sleep(forTimeInterval: 1.5)
            back.tap()
            Thread.sleep(forTimeInterval: 1.5)
        }) { return }
        measure(metrics: [XCTCPUMetric(application: app)], options: options) {
            startMeasuring()
            open.tap()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
            back.tap()
            Thread.sleep(forTimeInterval: settle)
        }
        open.tap()
        XCTAssertTrue(close.waitForExistence(timeout: 10), "the taps should still work")
    }

    // MARK: Scrolling: paging the result cards

    func testCheetahScroll() throws {
        scroll(cheetah())
    }

    func testNativeScroll() throws {
        scroll(try twin())
    }

    func testReactScroll() throws {
        scroll(try react())
    }

    func testTunedScroll() throws {
        scroll(try react(tuned: true))
    }

    private func scroll(_ app: XCUIApplication) {
        app.launch()
        let card = app.descendants(matching: .any)["card-java"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "result cards should be visible")
        sleep(2)
        // Along the text, below the photo's own pages.
        let width = springboard.frame.width
        let y = center(card, 0.5, 0.68).y
        let right = spot(CGPoint(x: width * 0.85, y: y), in: app)
        let left = spot(CGPoint(x: width * 0.15, y: y), in: app)
        let metrics: [XCTMetric] = [
            XCTOSSignpostMetric.scrollingAndDecelerationMetric,
            XCTCPUMetric(application: app),
        ]
        measure(metrics: metrics, options: options) {
            startMeasuring()
            for _ in 0..<3 {
                right.press(forDuration: 0.01, thenDragTo: left, withVelocity: .fast, thenHoldForDuration: 0)
                left.press(forDuration: 0.01, thenDragTo: right, withVelocity: .fast, thenHoldForDuration: 0)
            }
            stopMeasuring()
        }
    }

    // MARK: At rest, for what the rest is on top of

    func testCheetahIdle() throws {
        idle(cheetah())
    }

    func testNativeIdle() throws {
        idle(try twin())
    }

    func testReactIdle() throws {
        idle(try react())
    }

    func testTunedIdle() throws {
        idle(try react(tuned: true))
    }

    private func idle(_ app: XCUIApplication) {
        app.launch()
        XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 30), "a card should appear")
        sleep(3)
        let metrics: [XCTMetric] = [XCTCPUMetric(application: app), XCTMemoryMetric(application: app)]
        measure(metrics: metrics, options: options) {
            startMeasuring()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
        }
    }
}

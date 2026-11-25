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

    private static var waitsOff = false

    override class func setUp() {
        super.setUp()
        waitsOff = Quiescence.disable()
    }

    override class func tearDown() {
        Quiescence.restore()
        super.tearDown()
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    /// What was on screen when a test failed: an alert, the wrong screen.
    override func record(_ issue: XCTIssue) {
        let test = name.filter { $0.isLetter || $0.isNumber }
        print("CheetahPerf issue in \(test): \(issue.compactDescription)")
        if let detail = issue.detailedDescription {
            print("CheetahPerf detail: \(detail.prefix(3000))")
        }
        photograph("failed")
        super.record(issue)
    }

    /// To see that every version shows the same thing.
    private func photograph(_ what: String) {
        let test = name.filter { $0.isLetter || $0.isNumber }
        try? XCUIScreen.main.screenshot().pngRepresentation
            .write(to: URL(fileURLWithPath: "/tmp/cheetah-\(what)-\(test).png"))
    }

    /// `waitForExistence` looks about once a second, too coarse for timing.
    private func appears(_ element: XCUIElement, within seconds: TimeInterval) -> Bool {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if element.exists { return true }
            Thread.sleep(forTimeInterval: 0.02)
        }
        return false
    }

    /// Something over the app (a system banner a tap landed on) spoils the
    /// numbers: fail, and the run tries again.
    private func stillInFront(_ app: XCUIApplication) {
        XCTAssertEqual(app.state, .runningForeground, "the app should still be in front")
    }

    /// A part of the screen as a few gray levels. Taken from outside the app,
    /// which does nothing for it.
    private func look(_ frame: CGRect) -> [UInt8] {
        let image = XCUIScreen.main.screenshot().image
        let width = 48, height = 16
        var pixels = [UInt8](repeating: 0, count: width * height)
        let area = CGRect(
            x: frame.minX * image.scale, y: frame.minY * image.scale,
            width: frame.width * image.scale, height: frame.height * image.scale
        )
        guard let part = image.cgImage?.cropping(to: area) else { return pixels }
        pixels.withUnsafeMutableBytes { buffer in
            let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width, space: CGColorSpaceCreateDeviceGray(),
                bitmapInfo: CGImageAlphaInfo.none.rawValue
            )
            context?.draw(part, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        return pixels
    }

    private func same(_ a: [UInt8], _ b: [UInt8]) -> Bool {
        let difference = zip(a, b).reduce(0) { $0 + abs(Int($1.0) - Int($1.1)) }
        return difference / max(a.count, 1) < 6
    }

    /// An app that never goes idle can be measured like any other.
    func testNeverIdle() throws {
        XCTAssertTrue(Self.waitsOff, "XCTest's waiting should be off")
        let app = try twin(["--suggestions", "--busy"])
        app.launch()
        let suggestion = app.descendants(matching: .any)["Cafes within 12 min walk"].firstMatch
        XCTAssertTrue(appears(suggestion, within: 20), "suggestions should be visible")
        sleep(2)
        let show = spot(center(suggestion), in: app)
        let start = Date()
        show.tap()
        let took = Date().timeIntervalSince(start)
        XCTAssertTrue(appears(app.staticTexts["Java Haven"], within: 10), "the tap should work")
        XCTAssertLessThan(took, 5, "a tap shouldn't wait for the app to go idle")
        print("CheetahPerf never-idle tap took \(took) s")
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

    /// Cheetah's own signposts (HostMetrics): applying a frame's ops, and the
    /// layout and drawing after. Only Cheetah has them.
    private func metrics(_ app: XCUIApplication, host: Bool) -> [XCTMetric] {
        var metrics: [XCTMetric] = [XCTCPUMetric(application: app)]
        if host {
            metrics += ["apply", "commit"].map {
                XCTOSSignpostMetric(subsystem: "dev.cheetah.host", category: "frames", name: $0)
            }
        }
        return metrics
    }

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
        let title = app.staticTexts["Java Haven"].firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 30), "a card should appear")
        sleep(3)
        // Until the card's name looks as it does now.
        let frame = title.frame
        let shown = look(frame)
        measure(metrics: [XCTClockMetric()], options: options) {
            app.terminate()
            Thread.sleep(forTimeInterval: 1)
            XCTAssertFalse(same(look(frame), shown), "the card should be gone")
            startMeasuring()
            app.launch()
            let end = Date().addingTimeInterval(20)
            var appeared = false
            while !appeared, Date() < end {
                appeared = same(look(frame), shown)
            }
            stopMeasuring()
            XCTAssertTrue(appeared, "a card should appear")
        }
    }

    // MARK: Views changing

    /// A tap on a suggestion to its result card.
    func testCheetahResults() throws {
        results(cheetah("--nightshade"), host: true)
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

    private func results(_ app: XCUIApplication, host: Bool = false) {
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
        measure(metrics: metrics(app, host: host), options: options) {
            startMeasuring()
            show.tap()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
            stillInFront(app)
            reset.tap()
            Thread.sleep(forTimeInterval: settle)
        }
        show.tap()
        XCTAssertTrue(app.staticTexts["Java Haven"].waitForExistence(timeout: 10), "the taps should still work")
    }

    /// A tap on a card to its page.
    func testCheetahPage() throws {
        page(cheetah(), host: true)
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

    private func page(_ app: XCUIApplication, host: Bool = false) {
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
        measure(metrics: metrics(app, host: host), options: options) {
            startMeasuring()
            open.tap()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
            stillInFront(app)
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
        let next = { right.press(forDuration: 0.01, thenDragTo: left, withVelocity: .fast, thenHoldForDuration: 0) }
        let previous = { left.press(forDuration: 0.01, thenDragTo: right, withVelocity: .fast, thenHoldForDuration: 0) }

        // The swipes have to turn the page, or their cost means nothing.
        let frame = card.frame
        let text = CGRect(x: frame.minX + 20, y: y - 20, width: frame.width * 0.5, height: 50)
        let first = look(text)
        next()
        Thread.sleep(forTimeInterval: settle)
        photograph("second-card")
        XCTAssertFalse(same(look(text), first), "a swipe should show the next card")
        previous()
        Thread.sleep(forTimeInterval: settle)
        XCTAssertTrue(same(look(text), first), "a swipe back should show the first card")

        let metrics: [XCTMetric] = [
            XCTOSSignpostMetric.scrollingAndDecelerationMetric,
            XCTCPUMetric(application: app),
        ]
        measure(metrics: metrics, options: options) {
            startMeasuring()
            for _ in 0..<3 {
                next()
                Thread.sleep(forTimeInterval: 1.2)
                previous()
                Thread.sleep(forTimeInterval: 1.2)
            }
            stopMeasuring()
            stillInFront(app)
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
        // The map's tiles come over the network.
        sleep(10)
        photograph("idle")
        let metrics: [XCTMetric] = [XCTCPUMetric(application: app), XCTMemoryMetric(application: app)]
        measure(metrics: metrics, options: options) {
            startMeasuring()
            Thread.sleep(forTimeInterval: settle)
            stopMeasuring()
        }
    }
}

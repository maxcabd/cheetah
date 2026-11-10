import Foundation
import XCTest

/// Scriptable XCUITest adapter used by `cheetah-drive ios test`.
/// This test is skipped during normal UI test runs unless a script is supplied.
final class CheetahDriveTests: XCTestCase {
    private enum DriveError: Error, CustomStringConvertible {
        case message(String)

        var description: String {
            switch self {
            case .message(let value): return value
            }
        }
    }

    func testDriveScript() throws {
        let environment = ProcessInfo.processInfo.environment
        let scriptPath = environment["CHEETAH_DRIVE_FILE"]
            ?? "/tmp/cheetah-drive.script"
        let outputPath = environment["CHEETAH_DRIVE_OUT"]
            ?? "/tmp/cheetah-drive.out"

        guard FileManager.default.fileExists(atPath: scriptPath) else {
            throw XCTSkip("No cheetah drive script at \(scriptPath)")
        }

        let contents = try String(contentsOfFile: scriptPath, encoding: .utf8)
        var app: XCUIApplication?
        var operations: [[String: Any]] = []
        var latestTree: [[String: Any]] = []

        defer {
            let result: [String: Any] = [
                "nodes": latestTree,
                "operations": operations,
                "debug": app?.debugDescription ?? "",
            ]
            if let data = try? JSONSerialization.data(
                withJSONObject: result,
                options: [.prettyPrinted, .sortedKeys]
            ) {
                try? data.write(to: URL(fileURLWithPath: outputPath))
            }
        }

        for (index, rawLine) in contents.components(separatedBy: .newlines).enumerated() {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            if line.isEmpty || line.hasPrefix("#") {
                continue
            }

            let parts = splitQuoted(line)
            guard let command = parts.first else {
                continue
            }

            do {
                switch command {
                case "start":
                    app?.terminate()
                    let next = XCUIApplication()
                    switch parts.dropFirst().first ?? "nightshade" {
                    case "nightshade":
                        next.launchArguments = ["--nightshade"]
                    case "nightshade-demo":
                        next.launchArguments = ["--nightshade-demo"]
                    default:
                        next.launchArguments = []
                    }
                    next.launch()
                    app = next

                case "stop":
                    app?.terminate()
                    app = nil

                case "tree":
                    guard let app else {
                        throw DriveError.message("tree needs a started app")
                    }
                    latestTree = tree(app)

                case "tap":
                    guard let app else {
                        throw DriveError.message("tap needs a started app")
                    }
                    let target = try required(parts, 1, "tap target")
                    let element = awaitElement(app, target)
                    guard element.waitForExistence(timeout: 10) else {
                        throw DriveError.message("node \(target) not found")
                    }
                    element.tap()

                case "type":
                    guard let app else {
                        throw DriveError.message("type needs a started app")
                    }
                    let target = try required(parts, 1, "type target")
                    let text = parts.dropFirst(2).joined(separator: " ")
                    let element = awaitElement(app, target)
                    guard element.waitForExistence(timeout: 10) else {
                        throw DriveError.message("node \(target) not found")
                    }
                    element.tap()
                    element.typeText(text)

                case "expect":
                    guard let app else {
                        throw DriveError.message("expect needs a started app")
                    }
                    let kind = try required(parts, 1, "expect kind")
                    let target = try required(parts, 2, "expect target")
                    let element = awaitElement(app, target)
                    let exists = element.waitForExistence(timeout: 5)
                    switch kind {
                    case "exists" where exists:
                        break
                    case "missing" where !element.exists:
                        break
                    case "text" where exists:
                        break
                    default:
                        throw DriveError.message("expect \(kind) \(target) failed")
                    }

                case "wait":
                    let millis = UInt64(parts.dropFirst().first ?? "500") ?? 500
                    Thread.sleep(forTimeInterval: Double(millis) / 1000.0)

                case "settle":
                    guard let app else {
                        throw DriveError.message("settle needs a started app")
                    }
                    latestTree = tree(app)
                    Thread.sleep(forTimeInterval: 0.35)
                    latestTree = tree(app)

                case "clock":
                    let value = parts.dropFirst().first ?? "+500"
                    if value == "settle" {
                        guard let app else {
                            throw DriveError.message("clock settle needs a started app")
                        }
                        latestTree = tree(app)
                        Thread.sleep(forTimeInterval: 0.35)
                        latestTree = tree(app)
                    } else {
                        let millis = UInt64(value.replacingOccurrences(of: "+", with: "")) ?? 500
                        Thread.sleep(forTimeInterval: Double(millis) / 1000.0)
                    }

                case "screenshot":
                    let requested = try required(parts, 1, "screenshot path")
                    let base = environment["CHEETAH_DRIVE_CWD"]
                        ?? FileManager.default.currentDirectoryPath
                    let path = requested.hasPrefix("/")
                        ? requested
                        : base + "/" + requested
                    let url = URL(fileURLWithPath: path)
                    try FileManager.default.createDirectory(
                        at: url.deletingLastPathComponent(),
                        withIntermediateDirectories: true,
                        attributes: nil
                    )
                    try XCUIScreen.main.screenshot().pngRepresentation.write(to: url)

                default:
                    throw DriveError.message("unknown command \(command)")
                }

                operations.append(["line": index + 1, "operation": line, "ok": true])
            } catch {
                operations.append([
                    "line": index + 1,
                    "operation": line,
                    "ok": false,
                    "error": String(describing: error),
                ])
                throw error
            }
        }
    }

    private func required(
        _ parts: [String],
        _ index: Int,
        _ name: String
    ) throws -> String {
        guard let value = parts.dropFirst(index).first else {
            throw DriveError.message("missing \(name)")
        }
        return value
    }

    private func awaitElement(_ app: XCUIApplication, _ target: String) -> XCUIElement {
        let predicate = NSPredicate(
            format: "identifier == %@ OR label == %@ OR value == %@",
            target,
            target,
            target
        )
        return app.descendants(matching: .any).matching(predicate).firstMatch
    }

    private func tree(_ app: XCUIApplication) -> [[String: Any]] {
        app.descendants(matching: .any).allElementsBoundByIndex.map { element in
            let frame = element.frame
            return [
                "type": String(describing: element.elementType),
                "identifier": element.identifier,
                "label": element.label,
                "value": (element.value as? String) ?? "",
                "frame": [
                    "x": frame.origin.x,
                    "y": frame.origin.y,
                    "width": frame.size.width,
                    "height": frame.size.height,
                ],
                "enabled": element.isEnabled,
                "hittable": element.isHittable,
            ]
        }
    }

    private func splitQuoted(_ line: String) -> [String] {
        var values: [String] = []
        var current = ""
        var quote: Character?
        var escaped = false

        for character in line {
            if escaped {
                current.append(character)
                escaped = false
                continue
            }
            if character == "\\" && quote == "\"" {
                escaped = true
                continue
            }
            if let active = quote {
                if character == active {
                    quote = nil
                } else {
                    current.append(character)
                }
                continue
            }
            if character == "\"" || character == "'" {
                quote = character
                continue
            }
            if character.isWhitespace {
                if !current.isEmpty {
                    values.append(current)
                    current = ""
                }
                continue
            }
            current.append(character)
        }

        if !current.isEmpty {
            values.append(current)
        }
        return values
    }
}

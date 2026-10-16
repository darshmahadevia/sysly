import AppKit
import ApplicationServices

let args = CommandLine.arguments
guard args.count >= 3, let pid = pid_t(args[1]) else { fatalError("ui.swift PID dump|press|search|resize|close|screenshot [argument]") }
let application = AXUIElementCreateApplication(pid)

func activate() {
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n set frontmost of (first application process whose unix id is (item 1 of argv as integer)) to true\n end tell\nend run", String(pid)]
    do {
        try script.run()
        script.waitUntilExit()
        guard script.terminationStatus == 0 else { fatalError("Cannot activate app") }
    } catch {
        fatalError("Cannot activate app: \(error)")
    }
}

func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    AXUIElementCopyAttributeValue(element, name as CFString, &value)
    return value
}

func text(_ element: AXUIElement) -> String {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap { attribute(element, $0) as? String }.joined(separator: " | ")
}

func elements(_ root: AXUIElement, depth: Int = 0) -> [(AXUIElement, Int)] {
    if depth > 18 { return [] }
    let children = attribute(root, kAXChildrenAttribute) as? [AXUIElement] ?? []
    return [(root, depth)] + children.flatMap { elements($0, depth: depth + 1) }
}

func center(_ label: String, in all: [(AXUIElement, Int)]) -> CGPoint {
    guard let target = all.first(where: { text($0.0) == label })?.0,
          let position = attribute(target, kAXPositionAttribute), let size = attribute(target, kAXSizeAttribute) else { fatalError("Pointer target not found: \(label)") }
    var point = CGPoint.zero
    var dimensions = CGSize.zero
    AXValueGetValue(position as! AXValue, .cgPoint, &point)
    AXValueGetValue(size as! AXValue, .cgSize, &dimensions)
    return CGPoint(x: point.x + dimensions.width / 2, y: point.y + dimensions.height / 2)
}

func pointer(_ type: CGEventType, _ point: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
}

let all = elements(application)
switch args[2] {
case "tray":
    guard let target = all.first(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXMenuBarItemRole && text($0.0).contains("Sysly system monitor") })?.0 else { fatalError("Sysly menu bar logo not found") }
    let point = center(text(target), in: all)
    pointer(.mouseMoved, point)
    pointer(.leftMouseDown, point)
    Thread.sleep(forTimeInterval: 0.08)
    pointer(.leftMouseUp, point)
case "menu-dump":
    guard let bar = all.last(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXMenuBarRole })?.0 else { fatalError("Menu bar not found") }
    for (element, depth) in elements(bar) {
        let role = attribute(element, kAXRoleAttribute) as? String ?? "?"
        if [kAXMenuRole, kAXMenuItemRole, kAXStaticTextRole, kAXMenuBarRole, kAXMenuBarItemRole].contains(role) {
            print("\(String(repeating: " ", count: depth))\(role) \(text(element))")
        }
    }
case "menu-press":
    guard args.count > 3, let target = all.first(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXMenuItemRole && text($0.0).split(separator: "|").contains { $0.trimmingCharacters(in: .whitespaces) == args[3] } })?.0 else { fatalError("Menu action not found") }
    let point = center(text(target), in: all)
    pointer(.mouseMoved, point)
    pointer(.leftMouseDown, point)
    Thread.sleep(forTimeInterval: 0.08)
    pointer(.leftMouseUp, point)
case "menu-visible":
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    print(windows.contains(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 101 }))
case "menu-screenshot", "tray-screenshot":
    guard args.count > 3 else { fatalError("Missing output path") }
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 101 }), let id = window[kCGWindowNumber as String] as? Int else { fatalError("Open menu window not found") }
    let capture = Process()
    capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    if args[2] == "tray-screenshot" {
        guard let bounds = window[kCGWindowBounds as String] as? [String: Double],
              let x = bounds["X"], let y = bounds["Y"], let width = bounds["Width"], let height = bounds["Height"],
              let target = all.first(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXMenuBarItemRole && text($0.0).contains("Sysly system monitor") })?.0,
              let position = attribute(target, kAXPositionAttribute) else { fatalError("Tray bounds unavailable") }
        var point = CGPoint.zero
        AXValueGetValue(position as! AXValue, .cgPoint, &point)
        let top = min(y, point.y)
        let region = "\(Int(x)),\(Int(top)),\(Int(width)),\(Int(y + height - top))"
        capture.arguments = ["-x", "-o", "-R", region, args[3]]
    } else {
        capture.arguments = ["-x", "-o", "-l", String(id), args[3]]
    }
    try capture.run()
    capture.waitUntilExit()
    guard capture.terminationStatus == 0 else { fatalError("Menu screenshot failed") }
case "doctor":
    print("accessibility=\(AXIsProcessTrusted()) input=\(CGPreflightPostEventAccess()) screen=\(CGPreflightScreenCaptureAccess())")
case "visible":
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    print(windows.contains(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }))
case "bounds":
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }),
          let bounds = window[kCGWindowBounds as String] as? [String: Double] else { fatalError("Visible window not found") }
    print(String(data: try JSONSerialization.data(withJSONObject: bounds, options: [.sortedKeys]), encoding: .utf8)!)
case "key":
    guard args.count > 3 else { fatalError("Missing key") }
    let keys: [String: CGKeyCode] = ["1": 18, "2": 19, "3": 20, ",": 43, "f": 3, "q": 12, "a": 0, "escape": 53, "return": 36, "tab": 48, "space": 49, "delete": 51]
    guard let code = keys[args[3]] else { fatalError("Unsupported key") }
    let modifiers = args.dropFirst(4).map { modifier -> String in
        switch modifier {
        case "cmd": return "command down"
        case "shift": return "shift down"
        case "alt": return "option down"
        case "ctrl": return "control down"
        default: fatalError("Unsupported modifier")
        }
    }
    let suffix = modifiers.isEmpty ? "" : " using {" + modifiers.joined(separator: ", ") + "}"
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n set frontmost of (first application process whose unix id is (item 1 of argv as integer)) to true\n delay 0.2\n key code (item 2 of argv as integer)\(suffix)\n end tell\nend run", String(pid), String(code)]
    try script.run()
    script.waitUntilExit()
    guard script.terminationStatus == 0 else { fatalError("Keyboard input failed") }
case "type":
    guard args.count > 3 else { fatalError("Missing text") }
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n set frontmost of (first application process whose unix id is (item 1 of argv as integer)) to true\n keystroke (item 2 of argv)\n end tell\nend run", String(pid), args[3]]
    try script.run()
    script.waitUntilExit()
    guard script.terminationStatus == 0 else { fatalError("Text input failed") }
case "point", "scroll":
    guard args.count > 4, let x = Double(args[3]), let y = Double(args[4]) else { fatalError("Missing window-relative coordinates") }
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }),
          let bounds = window[kCGWindowBounds as String] as? [String: Double], let left = bounds["X"], let top = bounds["Y"] else { fatalError("Visible window not found") }
    activate()
    let point = CGPoint(x: left + x, y: top + y)
    pointer(.mouseMoved, point)
    Thread.sleep(forTimeInterval: 0.1)
    if args[2] == "point" {
        pointer(.leftMouseDown, point)
        Thread.sleep(forTimeInterval: 0.08)
        pointer(.leftMouseUp, point)
    } else {
        guard args.count > 5, let delta = Int32(args[5]) else { fatalError("Missing scroll distance") }
        CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0)?.post(tap: .cghidEventTap)
    }
case "file":
    guard args.count > 3 else { fatalError("Missing file path") }
    let deadline = Date().addingTimeInterval(5)
    while Date() < deadline {
        let windows = attribute(application, kAXWindowsAttribute) as? [AXUIElement] ?? []
        if windows.contains(where: { ["Open", "Save"].contains(text($0)) }) { break }
        Thread.sleep(forTimeInterval: 0.1)
    }
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n set frontmost of (first application process whose unix id is (item 1 of argv as integer)) to true\n keystroke \"g\" using {command down, shift down}\n delay 0.3\n keystroke (item 2 of argv)\n delay 0.6\n key code 36\n delay 0.6\n key code 36\n end tell\nend run", String(pid), args[3]]
    try script.run()
    script.waitUntilExit()
    guard script.terminationStatus == 0 else { fatalError("File selection failed") }
case "replace":
    guard args.count > 4 else { fatalError("Missing field value or replacement") }
    let fields = all.filter { attribute($0.0, kAXRoleAttribute) as? String == kAXTextFieldRole }
    let point = center(args[3], in: fields)
    activate()
    pointer(.mouseMoved, point)
    Thread.sleep(forTimeInterval: 0.1)
    pointer(.leftMouseDown, point)
    Thread.sleep(forTimeInterval: 0.08)
    pointer(.leftMouseUp, point)
    Thread.sleep(forTimeInterval: 0.15)
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n keystroke \"a\" using command down\n delay 0.1\n keystroke (item 1 of argv)\n end tell\nend run", args[4]]
    try script.run()
    script.waitUntilExit()
    guard script.terminationStatus == 0 else { fatalError("Text replacement failed") }
case "slider":
    guard args.count > 4, let number = Double(args[4]),
          let target = all.first(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXSliderRole && text($0.0) == args[3] })?.0 else { fatalError("Slider not found") }
    let result = AXUIElementSetAttributeValue(target, kAXValueAttribute as CFString, NSNumber(value: number))
    guard result == .success else { fatalError("Slider value failed: \(result)") }
case "dump":
    for (element, depth) in all {
        let role = attribute(element, kAXRoleAttribute) as? String ?? "?"
        print(String(repeating: " ", count: depth) + role + " " + text(element))
    }
case "press", "confirm":
    guard args.count > 3 else { fatalError("Missing control label") }
    let matches = all.filter { text($0.0).split(separator: "|").contains { $0.trimmingCharacters(in: .whitespaces) == args[3] } }
    guard let target = (args[2] == "confirm" ? matches.last : matches.first)?.0 else { fatalError("Control not found: \(args[3])") }
    let result = AXUIElementPerformAction(target, kAXPressAction as CFString)
    guard result == .success else { fatalError("Press failed: \(result)") }
case "search":
    guard args.count > 3 else { fatalError("Missing query") }
    guard let target = all.first(where: { attribute($0.0, kAXRoleAttribute) as? String == kAXTextFieldRole })?.0 else { fatalError("Search field not found") }
    _ = target
    let script = Process()
    script.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    script.arguments = ["-e", "on run argv\n tell application \"System Events\"\n set frontmost of (first application process whose unix id is (item 1 of argv as integer)) to true\n keystroke \"f\" using command down\n delay 0.2\n keystroke \"a\" using command down\n delay 0.1\n if (item 2 of argv) = \"\" then\n key code 51\n else\n keystroke (item 2 of argv)\n end if\n end tell\nend run", String(pid), args[3]]
    try script.run()
    script.waitUntilExit()
    guard script.terminationStatus == 0 else { fatalError("Search keystrokes failed") }

case "click":
    guard args.count > 3, let target = all.first(where: { text($0.0) == args[3] })?.0,
          let position = attribute(target, kAXPositionAttribute), let size = attribute(target, kAXSizeAttribute) else { fatalError("Click target not found") }
    var point = CGPoint.zero
    var dimensions = CGSize.zero
    AXValueGetValue(position as! AXValue, .cgPoint, &point)
    AXValueGetValue(size as! AXValue, .cgSize, &dimensions)
    point.x += dimensions.width / 2
    point.y += dimensions.height / 2
    print("click \(args[3]) at \(point.x),\(point.y)")
    activate()
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.1)
    for type in [CGEventType.leftMouseDown, CGEventType.leftMouseUp] {
        CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.08)
    }
case "drag", "drag-resize":
    guard args.count > 4 else { fatalError("Missing source or destination") }
    let source = center(args[3], in: all)
    let destination: CGPoint
    if args[2] == "drag-resize" {
        guard let delta = Double(args[4]) else { fatalError("Invalid resize delta") }
        destination = CGPoint(x: source.x, y: source.y + delta)
    } else {
        destination = center(args[4], in: all)
    }
    activate()
    pointer(.mouseMoved, source)
    Thread.sleep(forTimeInterval: 0.15)
    pointer(.leftMouseDown, source)
    Thread.sleep(forTimeInterval: 0.1)
    for index in 1...20 {
        let fraction = CGFloat(index) / 20
        pointer(.leftMouseDragged, CGPoint(x: source.x + (destination.x - source.x) * fraction, y: source.y + (destination.y - source.y) * fraction))
        Thread.sleep(forTimeInterval: 0.025)
    }
    pointer(.leftMouseUp, destination)
    Thread.sleep(forTimeInterval: 0.15)
case "resize":
    guard args.count > 4, let width = Double(args[3]), let height = Double(args[4]),
          let window = (attribute(application, kAXWindowsAttribute) as? [AXUIElement])?.first else { fatalError("Missing window or size") }
    var size = CGSize(width: width, height: height)
    let result = AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, AXValueCreate(.cgSize, &size)!)
    guard result == .success else { fatalError("Resize failed: \(result)") }
case "close":
    guard let window = (attribute(application, kAXWindowsAttribute) as? [AXUIElement])?.first,
          let button = attribute(window, kAXCloseButtonAttribute) else { fatalError("No close button") }
    let result = AXUIElementPerformAction(button as! AXUIElement, kAXPressAction as CFString)
    guard result == .success else { fatalError("Close failed: \(result)") }
case "screenshot":
    guard args.count > 3 else { fatalError("Missing output path") }
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    guard let window = windows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }), let id = window[kCGWindowNumber as String] as? Int else { fatalError("Visible window not found") }
    let capture = Process()
    capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    capture.arguments = ["-x", "-o", "-l", String(id), args[3]]
    try capture.run()
    capture.waitUntilExit()
    guard capture.terminationStatus == 0 else { fatalError("Screenshot failed") }
default:
    fatalError("Unknown operation")
}

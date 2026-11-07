
import AppKit

// Sysly's mark: three recorder lanes folded into an S, cut by the scrub cursor.
// The trace is recorded history. The stub past the cursor is its live end.
// All geometry lives here, on a 1024-unit grid, and every asset is drawn from it.
let destination = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "dist"
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let brand = root.appendingPathComponent("assets/brand")
let iconset = URL(fileURLWithPath: destination).appendingPathComponent("Sysly.iconset")
for directory in [brand, iconset] {
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
}

let stroke: CGFloat = 104
let turn: CGFloat = 95
let lanes: [CGFloat] = [322, 512, 702]
let leftTurn: CGFloat = 439, rightTurn: CGFloat = 585
let topEnd: CGFloat = 724, bottomEnd: CGFloat = 300
let cursor: CGFloat = 548
let tileInset: CGFloat = 100, tileRadius: CGFloat = 185
let traceColor = "#F2F3F5", liveColor = "#40C4BE"

enum Step {
    case line(CGFloat, CGFloat)
    case turn(CGFloat, CGFloat, clockwise: Bool)
}
struct Piece {
    let start: CGPoint
    let steps: [Step]
    var path: CGPath {
        let path = CGMutablePath()
        path.move(to: start)
        var from = start
        for step in steps {
            switch step {
            case let .line(x, y):
                path.addLine(to: CGPoint(x: x, y: y))
                from = CGPoint(x: x, y: y)
            case let .turn(x, y, clockwise):
                let center = CGPoint(x: x, y: (from.y + y) / 2)
                path.addArc(center: center, radius: abs(y - from.y) / 2, startAngle: -.pi / 2, endAngle: .pi / 2, clockwise: !clockwise)
                from = CGPoint(x: x, y: y)
            }
        }
        return path
    }
    var svg: String {
        var d = "M\(Int(start.x)) \(Int(start.y))"
        for step in steps {
            switch step {
            case let .line(x, y): d += " L\(Int(x)) \(Int(y))"
            case let .turn(x, y, clockwise): d += " A\(Int(turn)) \(Int(turn)) 0 0 \(clockwise ? 1 : 0) \(Int(x)) \(Int(y))"
            }
        }
        return d
    }
}
func pieces(gap: CGFloat) -> (trace: [Piece], live: [Piece]) {
    let left = cursor - gap / 2, right = cursor + gap / 2
    return (
        [
            Piece(start: CGPoint(x: left, y: lanes[0]), steps: [.line(leftTurn, lanes[0]), .turn(leftTurn, lanes[1], clockwise: false), .line(left, lanes[1])]),
            Piece(start: CGPoint(x: bottomEnd, y: lanes[2]), steps: [.line(left, lanes[2])]),
            Piece(start: CGPoint(x: right, y: lanes[1]), steps: [.line(rightTurn, lanes[1]), .turn(rightTurn, lanes[2], clockwise: true), .line(right, lanes[2])])
        ],
        [Piece(start: CGPoint(x: right, y: lanes[0]), steps: [.line(topEnd, lanes[0])])]
    )
}

let drawn = pieces(gap: 36)
func symbol(trace t: String, live l: String) -> String {
    func group(_ parts: [Piece], _ color: String) -> String {
        "<path d=\"\(parts.map(\.svg).joined(separator: " "))\" fill=\"none\" stroke=\"\(color)\" stroke-width=\"\(Int(stroke))\"/>"
    }
    return group(drawn.trace, t) + group(drawn.live, l)
}
func svg(_ contents: String, viewBox: String) -> String {
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"\(viewBox)\" role=\"img\" aria-label=\"Sysly logo\">\(contents)</svg>\n"
}
let markBox = "270 240 484 544"
let side = 1024 - tileInset * 2
let tileSVG = "<rect x=\"\(Int(tileInset))\" y=\"\(Int(tileInset))\" width=\"\(Int(side))\" height=\"\(Int(side))\" rx=\"\(Int(tileRadius))\" fill=\"#000\"/><rect x=\"\(Int(tileInset) + 2)\" y=\"\(Int(tileInset) + 2)\" width=\"\(Int(side) - 4)\" height=\"\(Int(side) - 4)\" rx=\"\(Int(tileRadius) - 2)\" fill=\"none\" stroke=\"#fff\" stroke-opacity=\".16\" stroke-width=\"4\"/>"
try svg(symbol(trace: traceColor, live: liveColor), viewBox: markBox).write(to: brand.appendingPathComponent("sysly-mark.svg"), atomically: true, encoding: .utf8)
try svg(symbol(trace: "currentColor", live: "currentColor"), viewBox: markBox).write(to: brand.appendingPathComponent("sysly-mark-mono.svg"), atomically: true, encoding: .utf8)
try svg(tileSVG + symbol(trace: traceColor, live: liveColor), viewBox: "0 0 1024 1024").write(to: brand.appendingPathComponent("sysly-icon.svg"), atomically: true, encoding: .utf8)

func color(_ hex: String) -> CGColor {
    let value = Int(hex.dropFirst(), radix: 16)!
    return CGColor(srgbRed: CGFloat(value >> 16) / 255, green: CGFloat(value >> 8 & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
}
func draw(_ parts: [Piece], in context: CGContext) {
    context.setLineWidth(stroke)
    context.setLineCap(.butt)
    context.setLineJoin(.round)
    for part in parts {
        context.addPath(part.path)
        context.strokePath()
    }
}
func render(pixels: Int, to url: URL) throws {
    let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: pixels * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let scale = CGFloat(pixels) / 1024
    context.translateBy(x: 0, y: CGFloat(pixels))
    context.scaleBy(x: scale, y: -scale)
    let box = CGRect(x: tileInset, y: tileInset, width: side, height: side)
    context.addPath(CGPath(roundedRect: box, cornerWidth: tileRadius, cornerHeight: tileRadius, transform: nil))
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fillPath()
    let edge = box.insetBy(dx: 2, dy: 2)
    context.addPath(CGPath(roundedRect: edge, cornerWidth: tileRadius - 2, cornerHeight: tileRadius - 2, transform: nil))
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.16))
    context.setLineWidth(4)
    context.strokePath()
    context.setStrokeColor(color(traceColor))
    draw(drawn.trace, in: context)
    context.setStrokeColor(color(liveColor))
    draw(drawn.live, in: context)
    let bitmap = NSBitmapImageRep(cgImage: context.makeImage()!)
    try bitmap.representation(using: .png, properties: [:])!.write(to: url)
}
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let suffix = scale == 2 ? "@2x" : ""
        try render(pixels: size * scale, to: iconset.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
}
try render(pixels: 1024, to: brand.appendingPathComponent("sysly-icon.png"))
print("Generated Sysly vector logos, 1024px icon, and macOS iconset.")

// The menu bar is one color, so the cursor gap widens to survive at 18 points.
let trayPixels = 36
let trayContext = CGContext(data: nil, width: trayPixels, height: trayPixels, bitsPerComponent: 8, bytesPerRow: trayPixels * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let trayScale = CGFloat(trayPixels - 4) / 484
trayContext.translateBy(x: 0, y: CGFloat(trayPixels))
trayContext.scaleBy(x: trayScale, y: -trayScale)
trayContext.translateBy(x: -270 + 2 / trayScale, y: -270 + 2 / trayScale)
trayContext.setStrokeColor(CGColor(gray: 0, alpha: 1))
let tray = pieces(gap: 44)
draw(tray.trace + tray.live, in: trayContext)
try Data(bytes: trayContext.data!, count: trayPixels * trayPixels * 4).write(to: brand.appendingPathComponent("sysly-tray.rgba"))

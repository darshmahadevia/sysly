import AppKit

let destination = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "dist"
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let brand = root.appendingPathComponent("assets/brand")
let iconset = URL(fileURLWithPath: destination).appendingPathComponent("Sysly.iconset")
for directory in [brand, iconset] {
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
}
let commands: [(String, [CGFloat])] = [
    ("M", [616, 300]), ("L", [432, 300]),
    ("C", [352, 300, 300, 344, 300, 408]),
    ("C", [300, 464, 336, 492, 408, 508]),
    ("L", [608, 552]),
    ("C", [680, 568, 716, 596, 716, 652]),
    ("C", [716, 716, 664, 760, 584, 760]),
    ("L", [368, 760])
]
let offset: CGFloat = -18
let path = CGMutablePath()
for (command, values) in commands {
    func p(_ index: Int) -> CGPoint { CGPoint(x: values[index], y: values[index + 1] + offset) }
    switch command {
    case "M": path.move(to: p(0))
    case "L": path.addLine(to: p(0))
    default: path.addCurve(to: p(4), control1: p(0), control2: p(2))
    }
}
let svgPath = commands.map { command, values in
    command + values.enumerated().map { index, value in String(Int(value + (index % 2 == 1 ? offset : 0))) }.joined(separator: " ")
}.joined(separator: " ")
let definitions = """
<defs>
  <linearGradient id="signal" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#83ABF8"/><stop offset=".52" stop-color="#64CDD7"/><stop offset="1" stop-color="#77D6B7"/></linearGradient>
  <linearGradient id="tile" x1="0" y1="0" x2=".7" y2="1"><stop stop-color="#2B3540"/><stop offset="1" stop-color="#131A22"/></linearGradient>
</defs>
"""
let symbol = "<path d=\"\(svgPath)\" fill=\"none\" stroke=\"url(#signal)\" stroke-width=\"88\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/><circle cx=\"744\" cy=\"282\" r=\"44\" fill=\"#77D6B7\"/>"
func svg(_ contents: String, viewBox: String = "0 0 1024 1024") -> String {
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"\(viewBox)\" role=\"img\" aria-label=\"Sysly live signal logo\">\(definitions)\(contents)</svg>\n"
}
try svg(symbol, viewBox: "216 198 616 628").write(to: brand.appendingPathComponent("sysly-mark.svg"), atomically: true, encoding: .utf8)
try svg("<rect x=\"48\" y=\"48\" width=\"928\" height=\"928\" rx=\"210\" fill=\"url(#tile)\"/>" + symbol).write(to: brand.appendingPathComponent("sysly-icon.svg"), atomically: true, encoding: .utf8)
try svg(symbol.replacingOccurrences(of: "url(#signal)", with: "currentColor").replacingOccurrences(of: "#77D6B7\"/>", with: "currentColor\"/>"), viewBox: "216 198 616 628").write(to: brand.appendingPathComponent("sysly-mark-mono.svg"), atomically: true, encoding: .utf8)
func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor {
    CGColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: 1)
}
func render(pixels: Int, to url: URL) throws {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: pixels * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let scale = CGFloat(pixels) / 1024
    context.translateBy(x: 0, y: CGFloat(pixels))
    context.scaleBy(x: scale, y: -scale)
    let tile = CGPath(roundedRect: CGRect(x: 48, y: 48, width: 928, height: 928), cornerWidth: 210, cornerHeight: 210, transform: nil)
    context.saveGState()
    context.addPath(tile)
    context.clip()
    let background = CGGradient(colorsSpace: space, colors: [color(43, 53, 64), color(19, 26, 34)] as CFArray, locations: [0, 1])!
    context.drawLinearGradient(background, start: CGPoint(x: 48, y: 48), end: CGPoint(x: 698, y: 976), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    context.restoreGState()
    context.saveGState()
    context.addPath(path.copy(strokingWithWidth: 88, lineCap: .round, lineJoin: .round, miterLimit: 10))
    context.clip()
    let signal = CGGradient(colorsSpace: space, colors: [color(131, 171, 248), color(100, 205, 215), color(119, 214, 183)] as CFArray, locations: [0, 0.52, 1])!
    context.drawLinearGradient(signal, start: CGPoint(x: 256, y: 238), end: CGPoint(x: 760, y: 786), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    context.restoreGState()
    context.setFillColor(color(119, 214, 183))
    context.fillEllipse(in: CGRect(x: 700, y: 238, width: 88, height: 88))
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

let trayPixels = 36
let trayContext = CGContext(data: nil, width: trayPixels, height: trayPixels, bitsPerComponent: 8, bytesPerRow: trayPixels * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
trayContext.translateBy(x: 0, y: CGFloat(trayPixels))
trayContext.scaleBy(x: CGFloat(trayPixels) / 628, y: -CGFloat(trayPixels) / 628)
trayContext.translateBy(x: -210, y: -198)
trayContext.setStrokeColor(CGColor(gray: 0, alpha: 1))
trayContext.setLineWidth(88)
trayContext.setLineCap(.round)
trayContext.setLineJoin(.round)
trayContext.addPath(path)
trayContext.strokePath()
trayContext.setFillColor(CGColor(gray: 0, alpha: 1))
trayContext.fillEllipse(in: CGRect(x: 700, y: 238, width: 88, height: 88))
try Data(bytes: trayContext.data!, count: trayPixels * trayPixels * 4).write(to: brand.appendingPathComponent("sysly-tray.rgba"))

import Foundation
import ImageIO
import Vision

guard CommandLine.arguments.count == 2 else {
    fputs("Usage: sysly-ocr screenshot.png\n", stderr)
    exit(2)
}
let url = URL(fileURLWithPath: CommandLine.arguments[1])
guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    fputs("Cannot read screenshot\n", stderr)
    exit(2)
}
let request = VNRecognizeTextRequest()
// Revision 3 fails to initialize its text runtime on the macOS 27 verification host.
request.revision = 2
request.recognitionLevel = .accurate
request.usesLanguageCorrection = false
try VNImageRequestHandler(cgImage: image).perform([request])
let rows: [[String: Any]] = (request.results ?? []).compactMap { observation in
    guard let candidate = observation.topCandidates(1).first else { return nil }
    let box = observation.boundingBox
    return [
        "text": candidate.string,
        "x": box.midX,
        "y": 1 - box.midY,
        "width": box.width,
        "height": box.height,
        "confidence": candidate.confidence,
    ]
}
let result: [String: Any] = ["width": image.width, "height": image.height, "lines": rows]
let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data("\n".utf8))

// Draw the background of the macOS disk image: a plain panel with an arrow
// from where the app icon sits to where the Applications folder sits.
// bundle.sh lays the two icons out on top of it at the positions below.
//
//   swift packaging/macos/background.swift packaging/macos/background.png
//
// The picture is drawn at twice the window size and saved at 144 dpi, so
// Finder shows it at 600 x 400 points, sharp on Retina screens.
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

// Window size in points and icon centers, the same numbers as in bundle.sh.
let windowWidth = 600.0
let windowHeight = 400.0
let appIconCenterX = 150.0
let applicationsIconCenterX = 450.0
let iconCenterY = 185.0
let iconSize = 128.0

let scale = 2.0
let backgroundColor = CGColor(red: 0.93, green: 0.93, blue: 0.95, alpha: 1)
let arrowColor = CGColor(red: 0.62, green: 0.62, blue: 0.67, alpha: 1)

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "background.png"
let width = Int(windowWidth * scale)
let height = Int(windowHeight * scale)
guard let context = CGContext(
    data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
    space: CGColorSpace(name: CGColorSpace.sRGB)!,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
else {
    fputs("could not make the drawing context\n", stderr)
    exit(1)
}
// Flip to the usual top-left origin so the y values above read like
// Finder's icon positions.
context.translateBy(x: 0, y: CGFloat(height))
context.scaleBy(x: scale, y: -scale)

context.setFillColor(backgroundColor)
context.fill(CGRect(x: 0, y: 0, width: windowWidth, height: windowHeight))

// The arrow spans the gap between the two icons, with a margin on each side.
let gap = 28.0
let arrowStartX = appIconCenterX + iconSize / 2 + gap
let arrowEndX = applicationsIconCenterX - iconSize / 2 - gap
let headLength = 22.0
let headHalfWidth = 16.0
context.setStrokeColor(arrowColor)
context.setLineWidth(7)
context.setLineCap(.round)
context.setLineJoin(.round)
context.move(to: CGPoint(x: arrowStartX, y: iconCenterY))
context.addLine(to: CGPoint(x: arrowEndX, y: iconCenterY))
context.move(to: CGPoint(x: arrowEndX - headLength, y: iconCenterY - headHalfWidth))
context.addLine(to: CGPoint(x: arrowEndX, y: iconCenterY))
context.addLine(to: CGPoint(x: arrowEndX - headLength, y: iconCenterY + headHalfWidth))
context.strokePath()

guard let image = context.makeImage(),
    let destination = CGImageDestinationCreateWithURL(
        URL(fileURLWithPath: output) as CFURL, UTType.png.identifier as CFString, 1, nil)
else {
    fputs("could not write \(output)\n", stderr)
    exit(1)
}
let dpi = 72.0 * scale
let properties: [CFString: Any] = [kCGImagePropertyDPIWidth: dpi, kCGImagePropertyDPIHeight: dpi]
CGImageDestinationAddImage(destination, image, properties as CFDictionary)
if !CGImageDestinationFinalize(destination) {
    fputs("could not write \(output)\n", stderr)
    exit(1)
}

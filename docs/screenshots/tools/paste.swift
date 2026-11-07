// Compose a staged screenshot: cut the title bar off one window capture
// and paste panels from other captures of the same window onto it. Each
// panel is clipped to its rounded rectangle and gets a soft shadow, so
// what surrounded it in its own capture does not come along. Everything
// is in points of the window (x right, y down from the top of the title
// bar); the captures are at whatever pixel scale the screen has.
//
//   swift paste.swift OUT BASE WIDTH HEIGHT [OVERLAY X Y W H DX DY]...
//
// OUT is the bottom HEIGHT points of BASE (a WIDTH points wide capture),
// with the W x H panel at (X, Y) of each OVERLAY drawn at (DX, DY).
import CoreGraphics
import Foundation
import ImageIO

/// Corner radius of the panels (the Ask bar and the summary card).
let cornerRadius = 8.0
let shadowBlur = 14.0, shadowOffset = 3.0, shadowAlpha = 0.45

let args = CommandLine.arguments
guard args.count >= 5, (args.count - 5) % 7 == 0 else {
    FileHandle.standardError.write("usage: paste.swift OUT BASE WIDTH HEIGHT [OVERLAY X Y W H DX DY]...\n".data(using: .utf8)!)
    exit(1)
}
let out = URL(fileURLWithPath: args[1])

func load(_ path: String) -> CGImage {
    guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        FileHandle.standardError.write("cannot read \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    return image
}

let base = load(args[2])
let pointsWidth = Double(args[3])!, pointsHeight = Double(args[4])!
// Pixels per point of the captures (2 on a Retina screen).
let scale = Double(base.width) / pointsWidth
let width = Int(pointsWidth * scale), height = Int(pointsHeight * scale)
let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                    space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
// Bottom-aligned, so the title bar overflows the top and is cut off.
ctx.draw(base, in: CGRect(x: 0, y: 0, width: CGFloat(base.width), height: CGFloat(base.height)))

var index = 5
while index < args.count {
    let overlay = load(args[index])
    let numbers = args[(index + 1)..<(index + 7)].map { Double($0)! }
    let (x, y, w, h, dx, dy) = (numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5])
    let cut = CGRect(x: x * scale, y: y * scale, width: w * scale, height: h * scale)
    guard let piece = overlay.cropping(to: cut) else {
        FileHandle.standardError.write("panel \(numbers) is outside \(args[index])\n".data(using: .utf8)!)
        exit(1)
    }
    // Core Graphics draws from the bottom: a point DY down from the top of
    // the full capture is at (capture height - DY - H) from its bottom.
    let bottom = Double(base.height) - (dy + h) * scale
    let target = CGRect(x: dx * scale, y: bottom, width: cut.width, height: cut.height)
    // The shadow applies to the layer as a whole, so it follows the rounded shape.
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -shadowOffset * scale), blur: shadowBlur * scale,
                  color: CGColor(red: 0, green: 0, blue: 0, alpha: shadowAlpha))
    ctx.beginTransparencyLayer(auxiliaryInfo: nil)
    ctx.addPath(CGPath(roundedRect: target, cornerWidth: cornerRadius * scale, cornerHeight: cornerRadius * scale, transform: nil))
    ctx.clip()
    ctx.draw(piece, in: target)
    ctx.endTransparencyLayer()
    ctx.restoreGState()
    index += 7
}

let result = ctx.makeImage()!
let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, result, nil)
CGImageDestinationFinalize(dest)
print("wrote \(out.path) \(width)x\(height)")

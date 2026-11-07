// Join five window captures into one image: strip i shows the x range
// i/5 of capture i, the theme name under it. Title bars (anything above the
// bottom `height` rows of a capture) are cut off.
// Usage: swift compose.swift out.png WIDTH HEIGHT label1=file1 label2=file2 ...
import CoreGraphics
import CoreText
import Foundation
import ImageIO

let args = CommandLine.arguments
let out = URL(fileURLWithPath: args[1])
let width = Int(args[2])!, height = Int(args[3])!
let parts: [(String, String)] = args[4...].map { let p = $0.split(separator: "=", maxSplits: 1); return (String(p[0]), String(p[1])) }
let labelBar = height / 20
let space = CGColorSpaceCreateDeviceRGB()
let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let strip = CGFloat(width) / CGFloat(parts.count)
for (index, (label, file)) in parts.enumerated() {
    let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: file) as CFURL, nil)!
    let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
    let x = strip * CGFloat(index)
    let rect = CGRect(x: x, y: 0, width: strip, height: CGFloat(height))
    ctx.saveGState()
    ctx.clip(to: rect)
    // Bottom-aligned: the capture's title bar overflows the top and is clipped.
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: CGFloat(image.width), height: CGFloat(image.height)))
    ctx.restoreGState()
    // The status bar color at the bottom of this strip, for the label bar.
    let data = ctx.data!.assumingMemoryBound(to: UInt8.self)
    // Inside the status bar, away from the rounded window corners.
    let row = height - labelBar / 2, col = Int(x) + Int(strip) / 2
    let offset = row * ctx.bytesPerRow + col * 4
    let r = CGFloat(data[offset]) / 255, g = CGFloat(data[offset + 1]) / 255, b = CGFloat(data[offset + 2]) / 255
    let fill = CGColor(red: r, green: g, blue: b, alpha: 1)
    let light = (0.299 * r + 0.587 * g + 0.114 * b) > 0.5
    ctx.setFillColor(fill)
    ctx.fill(CGRect(x: x, y: 0, width: strip, height: CGFloat(labelBar)))
    let textColor = light ? CGColor(red: 0.2, green: 0.2, blue: 0.22, alpha: 1) : CGColor(red: 0.85, green: 0.85, blue: 0.87, alpha: 1)
    let font = CTFontCreateWithName("Helvetica" as CFString, CGFloat(labelBar) * 0.42, nil)
    let text = NSAttributedString(string: label, attributes: [kCTFontAttributeName as NSAttributedString.Key: font, kCTForegroundColorAttributeName as NSAttributedString.Key: textColor])
    let line = CTLineCreateWithAttributedString(text)
    let textWidth = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
    ctx.textPosition = CGPoint(x: x + (strip - textWidth) / 2, y: CGFloat(labelBar) * 0.33)
    CTLineDraw(line, ctx)
}
let result = ctx.makeImage()!
let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, result, nil)
CGImageDestinationFinalize(dest)
print("wrote \(out.path) \(width)x\(height)")

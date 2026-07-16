// Renders the Tables app icon — a data grid glyph (a rounded table frame with a
// header divider and two column dividers) on a dark indigo squircle — to a
// 1024x1024 PNG. Pure CoreGraphics, no third-party deps, so it runs anywhere
// Swift does (local + CI macOS runners). scripts/icon.sh turns the PNG into the
// .iconset/.icns and a 512px downscale. Usage: swift scripts/icon.swift out.png
import CoreGraphics
import Foundation
import ImageIO

let outPath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icon.png"
let dim = 1024
let space = CGColorSpaceCreateDeviceRGB()

func color(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) -> CGColor {
  CGColor(colorSpace: space, components: [r, g, b, a])!
}

guard let ctx = CGContext(
  data: nil, width: dim, height: dim, bitsPerComponent: 8, bytesPerRow: 0,
  space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("could not create context") }

let full = CGFloat(dim)
ctx.clear(CGRect(x: 0, y: 0, width: full, height: full))

// Rounded-rect "squircle" body, inset from the canvas so the system shadow has
// room. Corner radius follows Apple's ~0.224 ratio of the body size.
let margin: CGFloat = 88
let body = CGRect(x: margin, y: margin, width: full - margin * 2, height: full - margin * 2)
let radius = body.width * 0.2237
let squircle = CGPath(roundedRect: body, cornerWidth: radius, cornerHeight: radius, transform: nil)

// Body fill: vertical indigo gradient (top lighter).
ctx.saveGState()
ctx.addPath(squircle)
ctx.clip()
let bodyGrad = CGGradient(
  colorsSpace: space,
  colors: [color(0.18, 0.16, 0.27), color(0.07, 0.07, 0.11)] as CFArray,
  locations: [0, 1]
)!
ctx.drawLinearGradient(
  bodyGrad, start: CGPoint(x: 0, y: full), end: CGPoint(x: 0, y: 0), options: []
)
// Soft sheen near the top for a little depth.
let sheen = CGGradient(
  colorsSpace: space,
  colors: [color(1, 1, 1, 0.10), color(1, 1, 1, 0)] as CFArray,
  locations: [0, 1]
)!
ctx.drawRadialGradient(
  sheen,
  startCenter: CGPoint(x: full / 2, y: full * 0.86), startRadius: 0,
  endCenter: CGPoint(x: full / 2, y: full * 0.86), endRadius: full * 0.6,
  options: []
)
ctx.restoreGState()

// Hairline top highlight on the rim.
ctx.saveGState()
ctx.addPath(squircle)
ctx.setStrokeColor(color(1, 1, 1, 0.06))
ctx.setLineWidth(3)
ctx.strokePath()
ctx.restoreGState()

// The data-grid glyph: a rounded table frame with a header divider and two
// column dividers. Origin is bottom-left.
let table = CGRect(x: 296, y: 332, width: 432, height: 360)
let frame = CGPath(
  roundedRect: table, cornerWidth: 30, cornerHeight: 30, transform: nil
)
let frameOutline = frame.copy(
  strokingWithWidth: 30, lineCap: .round, lineJoin: .round, miterLimit: 10
)

let lines = CGMutablePath()
// Header divider near the top.
let headerY: CGFloat = table.maxY - 96
lines.move(to: CGPoint(x: table.minX + 16, y: headerY))
lines.addLine(to: CGPoint(x: table.maxX - 16, y: headerY))
// Two column dividers.
for k in 1...2 {
  let x = table.minX + table.width * CGFloat(k) / 3
  lines.move(to: CGPoint(x: x, y: table.minY + 16))
  lines.addLine(to: CGPoint(x: x, y: table.maxY - 16))
}
let lineOutline = lines.copy(
  strokingWithWidth: 18, lineCap: .round, lineJoin: .round, miterLimit: 10
)

let glyph = CGMutablePath()
glyph.addPath(frameOutline)
glyph.addPath(lineOutline)

// Glow underneath for legibility on the dark body.
ctx.saveGState()
ctx.setShadow(offset: .zero, blur: 34, color: color(0.49, 0.82, 0.76, 0.55))
ctx.addPath(glyph)
ctx.setFillColor(color(0.49, 0.82, 0.76, 1))
ctx.fillPath()
ctx.restoreGState()

// Crisp teal-to-blue gradient fill on top of the glow.
ctx.saveGState()
ctx.addPath(glyph)
ctx.clip()
let glyphGrad = CGGradient(
  colorsSpace: space,
  colors: [color(0.55, 0.90, 0.80), color(0.42, 0.65, 1.0)] as CFArray,
  locations: [0, 1]
)!
ctx.drawLinearGradient(
  glyphGrad, start: CGPoint(x: 0, y: table.maxY), end: CGPoint(x: 0, y: table.minY), options: []
)
ctx.restoreGState()

guard let image = ctx.makeImage() else { fatalError("could not render image") }
let url = URL(fileURLWithPath: outPath)
guard let dest = CGImageDestinationCreateWithURL(
  url as CFURL, "public.png" as CFString, 1, nil
) else { fatalError("could not create \(outPath)") }
CGImageDestinationAddImage(dest, image, nil)
guard CGImageDestinationFinalize(dest) else { fatalError("could not write \(outPath)") }
print("wrote \(outPath)")

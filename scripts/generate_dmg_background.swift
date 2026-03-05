import AppKit
import Foundation

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
  fputs("Usage: generate_dmg_background.swift <output-path>\n", stderr)
  exit(1)
}

let outputPath = arguments[1]
let canvasSize = NSSize(width: 660, height: 400)
let canvasRect = NSRect(origin: .zero, size: canvasSize)

guard let bitmap = NSBitmapImageRep(
  bitmapDataPlanes: nil,
  pixelsWide: Int(canvasSize.width),
  pixelsHigh: Int(canvasSize.height),
  bitsPerSample: 8,
  samplesPerPixel: 4,
  hasAlpha: true,
  isPlanar: false,
  colorSpaceName: .deviceRGB,
  bytesPerRow: 0,
  bitsPerPixel: 0
) else {
  fputs("Failed to create bitmap canvas.\n", stderr)
  exit(1)
}

guard let graphicsContext = NSGraphicsContext(bitmapImageRep: bitmap) else {
  fputs("Failed to initialize graphics context.\n", stderr)
  exit(1)
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphicsContext

let gradient = NSGradient(
  colors: [
    NSColor(red: 0.04, green: 0.17, blue: 0.38, alpha: 1.0),
    NSColor(red: 0.12, green: 0.36, blue: 0.70, alpha: 1.0),
  ]
)!
gradient.draw(in: canvasRect, angle: 0)

let leftPanel = NSBezierPath(
  roundedRect: NSRect(x: 56, y: 94, width: 188, height: 188),
  xRadius: 20,
  yRadius: 20
)
NSColor(calibratedWhite: 1.0, alpha: 0.16).setFill()
leftPanel.fill()

let rightPanel = NSBezierPath(
  roundedRect: NSRect(x: 416, y: 94, width: 188, height: 188),
  xRadius: 20,
  yRadius: 20
)
NSColor(calibratedWhite: 1.0, alpha: 0.16).setFill()
rightPanel.fill()

let titleAttributes: [NSAttributedString.Key: Any] = [
  .font: NSFont.systemFont(ofSize: 34, weight: .bold),
  .foregroundColor: NSColor.white,
]
let title = "Drag ClawPal to Applications"
let titleRect = NSRect(x: 60, y: 314, width: 540, height: 44)
title.draw(in: titleRect, withAttributes: titleAttributes)

let subtitleAttributes: [NSAttributedString.Key: Any] = [
  .font: NSFont.systemFont(ofSize: 16, weight: .medium),
  .foregroundColor: NSColor(calibratedWhite: 1.0, alpha: 0.9),
]
let subtitle = "Drop ClawPal.app into Applications to install"
let subtitleRect = NSRect(x: 60, y: 286, width: 540, height: 24)
subtitle.draw(in: subtitleRect, withAttributes: subtitleAttributes)

let arrowPath = NSBezierPath()
arrowPath.lineWidth = 12
arrowPath.lineCapStyle = .round
arrowPath.move(to: NSPoint(x: 254, y: 188))
arrowPath.line(to: NSPoint(x: 396, y: 188))
NSColor(calibratedWhite: 1.0, alpha: 0.92).setStroke()
arrowPath.stroke()

let arrowHead = NSBezierPath()
arrowHead.move(to: NSPoint(x: 396, y: 222))
arrowHead.line(to: NSPoint(x: 452, y: 188))
arrowHead.line(to: NSPoint(x: 396, y: 154))
arrowHead.close()
NSColor(calibratedWhite: 1.0, alpha: 0.92).setFill()
arrowHead.fill()

let badgeAttributes: [NSAttributedString.Key: Any] = [
  .font: NSFont.systemFont(ofSize: 16, weight: .semibold),
  .foregroundColor: NSColor.white,
]
"ClawPal.app".draw(
  in: NSRect(x: 88, y: 56, width: 164, height: 24),
  withAttributes: badgeAttributes
)
"Applications".draw(
  in: NSRect(x: 440, y: 56, width: 164, height: 24),
  withAttributes: badgeAttributes
)

NSGraphicsContext.restoreGraphicsState()

guard let pngData = bitmap.representation(using: .png, properties: [:]) else {
  fputs("Failed to encode PNG data.\n", stderr)
  exit(1)
}

let outputURL = URL(fileURLWithPath: outputPath)
do {
  try FileManager.default.createDirectory(
    at: outputURL.deletingLastPathComponent(),
    withIntermediateDirectories: true,
    attributes: nil
  )
  try pngData.write(to: outputURL, options: .atomic)
  print("Wrote DMG background to \(outputURL.path)")
} catch {
  fputs("Failed to write image: \(error)\n", stderr)
  exit(1)
}

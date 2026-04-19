#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
ICON_ROOT="${PROJECT_ROOT}/assets/icon"
MASTER_PNG="${ICON_ROOT}/AppIcon-master.png"
ICONSET_DIR="${ICON_ROOT}/AppIcon.iconset"
ICNS_PATH="${ICON_ROOT}/AppIcon.icns"

mkdir -p "${ICON_ROOT}"
rm -rf "${ICONSET_DIR}"
mkdir -p "${ICONSET_DIR}"

SWIFT_SCRIPT="$(mktemp)"
cat > "${SWIFT_SCRIPT}" <<'SWIFT'
import AppKit

let args = CommandLine.arguments
if args.count < 2 {
    fputs("Usage: swift icon.swift <output-png>\n", stderr)
    exit(1)
}

let outputURL = URL(fileURLWithPath: args[1])
let canvasSize = CGSize(width: 1024, height: 1024)
let image = NSImage(size: NSSize(width: canvasSize.width, height: canvasSize.height))

image.lockFocus()
guard let context = NSGraphicsContext.current?.cgContext else {
    fputs("Unable to acquire graphics context\n", stderr)
    exit(2)
}

let rect = CGRect(origin: .zero, size: canvasSize)
let cornerRadius: CGFloat = 228

context.setAllowsAntialiasing(true)
context.setShouldAntialias(true)

let clip = CGPath(roundedRect: rect, cornerWidth: cornerRadius, cornerHeight: cornerRadius, transform: nil)
context.addPath(clip)
context.clip()

let background = CGGradient(
    colorsSpace: CGColorSpaceCreateDeviceRGB(),
    colors: [
        CGColor(red: 0.015, green: 0.025, blue: 0.09, alpha: 1.0),
        CGColor(red: 0.03, green: 0.07, blue: 0.22, alpha: 1.0),
        CGColor(red: 0.05, green: 0.17, blue: 0.38, alpha: 1.0)
    ] as CFArray,
    locations: [0.0, 0.58, 1.0]
)!

context.drawLinearGradient(background, start: CGPoint(x: 0, y: 1024), end: CGPoint(x: 1024, y: 0), options: [])

let deepSpace = CGGradient(
    colorsSpace: CGColorSpaceCreateDeviceRGB(),
    colors: [
        CGColor(red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0),
        CGColor(red: 0.0, green: 0.0, blue: 0.0, alpha: 0.5)
    ] as CFArray,
    locations: [0.0, 1.0]
)!
context.drawRadialGradient(
    deepSpace,
    startCenter: CGPoint(x: 700, y: 860),
    startRadius: 20,
    endCenter: CGPoint(x: 700, y: 860),
    endRadius: 900,
    options: [.drawsAfterEndLocation]
)

func drawOrbit(center: CGPoint, radius: CGFloat, lineWidth: CGFloat, alpha: CGFloat) {
    let orbit = CGPath(ellipseIn: CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2), transform: nil)
    context.addPath(orbit)
    context.setStrokeColor(CGColor(red: 0.62, green: 0.8, blue: 1.0, alpha: alpha))
    context.setLineWidth(lineWidth)
    context.strokePath()
}

let orbitCenter = CGPoint(x: 470, y: 510)
drawOrbit(center: orbitCenter, radius: 168, lineWidth: 12, alpha: 0.45)
drawOrbit(center: orbitCenter, radius: 252, lineWidth: 10, alpha: 0.34)
drawOrbit(center: orbitCenter, radius: 338, lineWidth: 8, alpha: 0.26)

let sunCoreCenter = CGPoint(x: 420, y: 560)
let sunCoreRadius: CGFloat = 124
let sunGlow = CGGradient(
    colorsSpace: CGColorSpaceCreateDeviceRGB(),
    colors: [
        CGColor(red: 1.0, green: 0.78, blue: 0.16, alpha: 0.92),
        CGColor(red: 0.98, green: 0.36, blue: 0.12, alpha: 0.0)
    ] as CFArray,
    locations: [0.0, 1.0]
)!
context.drawRadialGradient(
    sunGlow,
    startCenter: sunCoreCenter,
    startRadius: 8,
    endCenter: sunCoreCenter,
    endRadius: 260,
    options: [.drawsAfterEndLocation]
)

let sunDisk = CGPath(ellipseIn: CGRect(x: sunCoreCenter.x - sunCoreRadius, y: sunCoreCenter.y - sunCoreRadius, width: sunCoreRadius * 2, height: sunCoreRadius * 2), transform: nil)
context.addPath(sunDisk)
context.setFillColor(CGColor(red: 1.0, green: 0.83, blue: 0.24, alpha: 1.0))
context.fillPath()

let sunInner = CGPath(ellipseIn: CGRect(x: sunCoreCenter.x - 68, y: sunCoreCenter.y - 68, width: 136, height: 136), transform: nil)
context.addPath(sunInner)
context.setFillColor(CGColor(red: 1.0, green: 0.93, blue: 0.56, alpha: 0.9))
context.fillPath()

func drawPlanet(at point: CGPoint, radius: CGFloat, color: CGColor, glow: CGColor) {
    let halo = CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(),
        colors: [
            glow,
            CGColor(red: 0.0, green: 0.0, blue: 0.0, alpha: 0.0)
        ] as CFArray,
        locations: [0.0, 1.0]
    )!

    context.drawRadialGradient(
        halo,
        startCenter: point,
        startRadius: 2,
        endCenter: point,
        endRadius: radius * 5,
        options: [.drawsAfterEndLocation]
    )

    let disk = CGPath(ellipseIn: CGRect(x: point.x - radius, y: point.y - radius, width: radius * 2, height: radius * 2), transform: nil)
    context.addPath(disk)
    context.setFillColor(color)
    context.fillPath()
}

drawPlanet(
    at: CGPoint(x: 664, y: 690),
    radius: 24,
    color: CGColor(red: 0.4, green: 0.72, blue: 1.0, alpha: 1.0),
    glow: CGColor(red: 0.45, green: 0.76, blue: 1.0, alpha: 0.5)
)

drawPlanet(
    at: CGPoint(x: 758, y: 474),
    radius: 34,
    color: CGColor(red: 0.98, green: 0.55, blue: 0.25, alpha: 1.0),
    glow: CGColor(red: 1.0, green: 0.48, blue: 0.22, alpha: 0.45)
)

drawPlanet(
    at: CGPoint(x: 595, y: 274),
    radius: 18,
    color: CGColor(red: 0.79, green: 0.88, blue: 1.0, alpha: 1.0),
    glow: CGColor(red: 0.65, green: 0.82, blue: 1.0, alpha: 0.4)
)

for i in 0..<160 {
    let x = CGFloat((i * 71) % 1024)
    let y = CGFloat((i * 197) % 1024)
    let brightness = 0.6 + CGFloat((i * 17) % 40) / 100.0
    let size = CGFloat(1 + (i % 3))
    let starRect = CGRect(x: x, y: y, width: size, height: size)
    context.setFillColor(CGColor(red: brightness, green: brightness, blue: brightness, alpha: 0.8))
    context.fillEllipse(in: starRect)
}

image.unlockFocus()

guard
    let tiffData = image.tiffRepresentation,
    let rep = NSBitmapImageRep(data: tiffData),
    let pngData = rep.representation(using: .png, properties: [:])
else {
    fputs("Unable to export PNG\n", stderr)
    exit(3)
}

try pngData.write(to: outputURL)
SWIFT

swift "${SWIFT_SCRIPT}" "${MASTER_PNG}"
rm -f "${SWIFT_SCRIPT}"
sips -z 1024 1024 "${MASTER_PNG}" --out "${MASTER_PNG}" >/dev/null

# size filename
while read -r size name; do
  sips -z "${size}" "${size}" "${MASTER_PNG}" --out "${ICONSET_DIR}/${name}" >/dev/null
done <<'SIZES'
16 icon_16x16.png
32 icon_16x16@2x.png
32 icon_32x32.png
64 icon_32x32@2x.png
128 icon_128x128.png
256 icon_128x128@2x.png
256 icon_256x256.png
512 icon_256x256@2x.png
512 icon_512x512.png
1024 icon_512x512@2x.png
SIZES

iconutil -c icns "${ICONSET_DIR}" -o "${ICNS_PATH}"
echo "Generated ${ICNS_PATH}"

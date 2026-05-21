#!/usr/bin/env swift
import Foundation
import CoreGraphics
import ImageIO
import CoreText
import UniformTypeIdentifiers
import AppKit

// MARK: - Output configuration

struct IconSize {
    let pixels: Int
    let filename: String
    /// macOS icons follow the HIG convention of a squircle inset within a transparent
    /// canvas (roughly 10% padding). iOS expects a full-bleed image that the system masks.
    let insetForMac: Bool
}

let outputDir = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first
    ?? "ios/Sources/TempMail/Assets.xcassets/AppIcon.appiconset")

let sizes: [IconSize] = [
    // iOS universal — full bleed, Xcode applies the mask
    IconSize(pixels: 1024, filename: "AppIcon-iOS-1024.png", insetForMac: false),
    // macOS — provide the squircle with transparent padding ourselves
    IconSize(pixels: 16,   filename: "AppIcon-mac-16.png",        insetForMac: true),
    IconSize(pixels: 32,   filename: "AppIcon-mac-16@2x.png",     insetForMac: true),
    IconSize(pixels: 32,   filename: "AppIcon-mac-32.png",        insetForMac: true),
    IconSize(pixels: 64,   filename: "AppIcon-mac-32@2x.png",     insetForMac: true),
    IconSize(pixels: 128,  filename: "AppIcon-mac-128.png",       insetForMac: true),
    IconSize(pixels: 256,  filename: "AppIcon-mac-128@2x.png",    insetForMac: true),
    IconSize(pixels: 256,  filename: "AppIcon-mac-256.png",       insetForMac: true),
    IconSize(pixels: 512,  filename: "AppIcon-mac-256@2x.png",    insetForMac: true),
    IconSize(pixels: 512,  filename: "AppIcon-mac-512.png",       insetForMac: true),
    IconSize(pixels: 1024, filename: "AppIcon-mac-512@2x.png",    insetForMac: true),
]

// MARK: - Drawing

/// Squircle path (superellipse) approximating Apple's icon shape.
func squirclePath(in rect: CGRect) -> CGPath {
    // Use a continuous corner radius approximation via a relative corner radius.
    let radius = rect.width * 0.2237   // ~Apple's mask ratio for app icons
    return CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

func drawSparkle(at center: CGPoint, size: CGFloat, in ctx: CGContext, color: CGColor) {
    let path = CGMutablePath()
    let inner = size * 0.25
    // 4-point star (sparkle): tall vertical, slim horizontal
    path.move(to:    CGPoint(x: center.x,         y: center.y + size))
    path.addQuadCurve(to: CGPoint(x: center.x + inner, y: center.y + inner),
                      control: CGPoint(x: center.x + inner * 0.4, y: center.y + inner * 0.4 + size * 0.15))
    path.addLine(to: CGPoint(x: center.x + size,  y: center.y))
    path.addQuadCurve(to: CGPoint(x: center.x + inner, y: center.y - inner),
                      control: CGPoint(x: center.x + inner * 0.4, y: center.y - inner * 0.4 - size * 0.15))
    path.addLine(to: CGPoint(x: center.x,         y: center.y - size))
    path.addQuadCurve(to: CGPoint(x: center.x - inner, y: center.y - inner),
                      control: CGPoint(x: center.x - inner * 0.4, y: center.y - inner * 0.4 - size * 0.15))
    path.addLine(to: CGPoint(x: center.x - size,  y: center.y))
    path.addQuadCurve(to: CGPoint(x: center.x - inner, y: center.y + inner),
                      control: CGPoint(x: center.x - inner * 0.4, y: center.y + inner * 0.4 + size * 0.15))
    path.closeSubpath()

    ctx.addPath(path)
    ctx.setFillColor(color)
    ctx.fillPath()
}

func renderIcon(pixels: Int, insetForMac: Bool) -> CGImage? {
    let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    guard let ctx = CGContext(data: nil,
                              width: pixels,
                              height: pixels,
                              bitsPerComponent: 8,
                              bytesPerRow: 0,
                              space: colorSpace,
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
        return nil
    }

    // Transparent background
    ctx.clear(CGRect(x: 0, y: 0, width: pixels, height: pixels))

    // For Mac, inset the squircle ~10% on each side so it sits inside transparent padding.
    let canvas = CGRect(x: 0, y: 0, width: pixels, height: pixels)
    let iconRect: CGRect = insetForMac
        ? canvas.insetBy(dx: CGFloat(pixels) * 0.10, dy: CGFloat(pixels) * 0.10)
        : canvas

    let squircle = squirclePath(in: iconRect)

    // Draw a subtle drop shadow under the squircle (mostly visible on macOS where there's padding)
    if insetForMac {
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: -CGFloat(pixels) * 0.012),
                      blur: CGFloat(pixels) * 0.025,
                      color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.25))
        ctx.addPath(squircle)
        ctx.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        ctx.fillPath()
        ctx.restoreGState()
    }

    // Clip to the squircle and paint the gradient
    ctx.saveGState()
    ctx.addPath(squircle)
    ctx.clip()

    // Blue → purple diagonal gradient
    let colors = [
        CGColor(red: 0.30, green: 0.55, blue: 1.00, alpha: 1.0),  // bright blue
        CGColor(red: 0.45, green: 0.30, blue: 0.95, alpha: 1.0),  // indigo
        CGColor(red: 0.65, green: 0.25, blue: 0.90, alpha: 1.0),  // violet
    ] as CFArray
    let locations: [CGFloat] = [0.0, 0.55, 1.0]
    let gradient = CGGradient(colorsSpace: colorSpace, colors: colors, locations: locations)!
    ctx.drawLinearGradient(gradient,
                           start: CGPoint(x: iconRect.minX, y: iconRect.maxY),
                           end:   CGPoint(x: iconRect.maxX, y: iconRect.minY),
                           options: [])

    // Soft inner highlight (top-left glow) to lean into the Liquid Glass look
    let highlightColors = [
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.35),
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.0),
    ] as CFArray
    let highlightGradient = CGGradient(colorsSpace: colorSpace, colors: highlightColors,
                                       locations: [0.0, 1.0])!
    ctx.drawRadialGradient(highlightGradient,
                           startCenter: CGPoint(x: iconRect.minX + iconRect.width * 0.25,
                                                y: iconRect.maxY - iconRect.height * 0.20),
                           startRadius: 0,
                           endCenter: CGPoint(x: iconRect.minX + iconRect.width * 0.25,
                                              y: iconRect.maxY - iconRect.height * 0.20),
                           endRadius: iconRect.width * 0.55,
                           options: [])

    // Sparkles around the @ glyph
    let sparkleColor = CGColor(red: 1, green: 1, blue: 1, alpha: 0.9)
    let cx = iconRect.midX
    let cy = iconRect.midY
    let unit = iconRect.width
    drawSparkle(at: CGPoint(x: cx + unit * 0.27, y: cy + unit * 0.27),
                size: unit * 0.075, in: ctx, color: sparkleColor)
    drawSparkle(at: CGPoint(x: cx - unit * 0.30, y: cy + unit * 0.05),
                size: unit * 0.045, in: ctx, color: sparkleColor)
    drawSparkle(at: CGPoint(x: cx + unit * 0.05, y: cy - unit * 0.32),
                size: unit * 0.050, in: ctx, color: sparkleColor)

    // The @ glyph, centered
    let fontSize = unit * 0.72
    // Prefer SF Pro Rounded Bold; fall back to system bold
    let font: CTFont = {
        let desc = CTFontDescriptorCreateWithAttributes([
            kCTFontFamilyNameAttribute: "SF Pro Rounded",
            kCTFontTraitsAttribute: [kCTFontWeightTrait: 0.6]
        ] as CFDictionary)
        let f = CTFontCreateWithFontDescriptor(desc, fontSize, nil)
        let name = CTFontCopyFamilyName(f) as String
        if name.contains("Rounded") { return f }
        return CTFontCreateUIFontForLanguage(.system, fontSize, nil) ?? f
    }()

    let attr: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: NSColor.white,
    ]
    let attrString = NSAttributedString(string: "@", attributes: attr)
    let line = CTLineCreateWithAttributedString(attrString)
    let bounds = CTLineGetBoundsWithOptions(line, [.useOpticalBounds])
    let textX = cx - bounds.width / 2 - bounds.origin.x
    let textY = cy - bounds.height / 2 - bounds.origin.y

    // Soft glow behind the @
    ctx.saveGState()
    ctx.setShadow(offset: .zero,
                  blur: unit * 0.03,
                  color: CGColor(red: 1, green: 1, blue: 1, alpha: 0.6))
    ctx.textPosition = CGPoint(x: textX, y: textY)
    CTLineDraw(line, ctx)
    ctx.restoreGState()

    // The @ itself, crisp
    ctx.textPosition = CGPoint(x: textX, y: textY)
    CTLineDraw(line, ctx)

    ctx.restoreGState()

    // Hairline rim highlight along the top edge of the squircle (Liquid Glass cue)
    ctx.saveGState()
    ctx.addPath(squircle)
    ctx.clip()
    let rimColors = [
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.5),
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.0),
    ] as CFArray
    let rimGradient = CGGradient(colorsSpace: colorSpace, colors: rimColors,
                                 locations: [0.0, 1.0])!
    ctx.drawLinearGradient(rimGradient,
                           start: CGPoint(x: iconRect.midX, y: iconRect.maxY),
                           end:   CGPoint(x: iconRect.midX, y: iconRect.maxY - iconRect.height * 0.08),
                           options: [])
    ctx.restoreGState()

    return ctx.makeImage()
}

// MARK: - Write PNGs

func writePNG(_ image: CGImage, to url: URL) throws {
    guard let dest = CGImageDestinationCreateWithURL(url as CFURL,
                                                     UTType.png.identifier as CFString,
                                                     1, nil) else {
        throw NSError(domain: "icon", code: 1)
    }
    CGImageDestinationAddImage(dest, image, nil)
    if !CGImageDestinationFinalize(dest) {
        throw NSError(domain: "icon", code: 2)
    }
}

try FileManager.default.createDirectory(at: outputDir, withIntermediateDirectories: true)

for size in sizes {
    guard let image = renderIcon(pixels: size.pixels, insetForMac: size.insetForMac) else {
        FileHandle.standardError.write("failed to render \(size.filename)\n".data(using: .utf8)!)
        continue
    }
    let url = outputDir.appendingPathComponent(size.filename)
    try writePNG(image, to: url)
    print("wrote \(url.path) (\(size.pixels)x\(size.pixels))")
}

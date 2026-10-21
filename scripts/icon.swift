// Renders the app icon: "txt" set in Georgia on Margin's sage, 1024 px.
import AppKit
let size = 1024.0
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let inset = 100.0, rect = NSRect(x: inset, y: inset, width: size - 2 * inset, height: size - 2 * inset)
NSColor(red: 0.965, green: 0.961, blue: 0.945, alpha: 1).setFill()
NSBezierPath(roundedRect: rect, xRadius: 185, yRadius: 185).fill()
let attrs: [NSAttributedString.Key: Any] = [.font: NSFont(name: "Georgia", size: 330)!, .foregroundColor: NSColor(red: 0.22, green: 0.341, blue: 0.278, alpha: 1), .kern: -14]
let text = NSAttributedString(string: "txt", attributes: attrs)
let t = text.size()
text.draw(at: NSPoint(x: (size - t.width) / 2, y: (size - t.height) / 2 + 20))
NSColor(red: 0.22, green: 0.341, blue: 0.278, alpha: 1).setFill()
NSRect(x: (size + t.width) / 2 + 8, y: size / 2 - 105, width: 14, height: 250).fill()
image.unlockFocus()
let rep = NSBitmapImageRep(data: image.tiffRepresentation!)!
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))

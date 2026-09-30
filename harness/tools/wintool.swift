// wintool list PID...   one line per window of those PIDs: "id layer onscreen width height name"
// wintool park PID...   move the mouse pointer to a screen corner outside the largest window of those PIDs, so no
//                       widget shows a hover state in a screenshot
import CoreGraphics
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
guard let mode = args.first else { exit(2) }
let pids = Set(args.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
var best: CGRect? = nil
for w in list {
    guard let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p) else { continue }
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let rect = CGRect(x: (b["X"] as? Double) ?? 0, y: (b["Y"] as? Double) ?? 0,
                      width: (b["Width"] as? Double) ?? 0, height: (b["Height"] as? Double) ?? 0)
    if mode == "list" {
        print(w[kCGWindowNumber as String] ?? "?", w[kCGWindowLayer as String] ?? "?",
              w[kCGWindowIsOnscreen as String] ?? "no", rect.width, rect.height, w[kCGWindowName as String] ?? "")
    }
    if best == nil || rect.width * rect.height > best!.width * best!.height { best = rect }
}
if mode == "park" {
    let d = CGDisplayBounds(CGMainDisplayID())
    let corners = [CGPoint(x: d.minX + 1, y: d.minY + 1), CGPoint(x: d.maxX - 2, y: d.minY + 1),
                   CGPoint(x: d.minX + 1, y: d.maxY - 2), CGPoint(x: d.maxX - 2, y: d.maxY - 2)]
    let free = corners.first { c in best == nil || !best!.contains(c) } ?? corners[0]
    CGWarpMouseCursorPosition(free)
    print("parked at \(free.x) \(free.y)")
}

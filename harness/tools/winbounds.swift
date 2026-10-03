// winbounds PID...   one line per window of those PIDs: "id layer onscreen x y w h name" (points, top-left origin).
// Read-only. Used by tools/record_window.py to crop a full-screen capture to the game window.
import CoreGraphics
import Foundation
let pids = Set(CommandLine.arguments.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    guard let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p) else { continue }
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    print(w[kCGWindowNumber as String] ?? "?", w[kCGWindowLayer as String] ?? "?", w[kCGWindowIsOnscreen as String] ?? "no",
          b["X"] ?? "?", b["Y"] ?? "?", b["Width"] ?? "?", b["Height"] ?? "?", w[kCGWindowName as String] ?? "")
}

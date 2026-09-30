// wintool list PID...   one line per window of those PIDs: "id layer onscreen width height name"
// Read-only: it lists windows so the gate can capture only windows it launched (screencapture -l <id>).
import CoreGraphics
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
let pids = Set(args.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    guard let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p) else { continue }
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    print(w[kCGWindowNumber as String] ?? "?", w[kCGWindowLayer as String] ?? "?",
          w[kCGWindowIsOnscreen as String] ?? "no", b["Width"] ?? "?", b["Height"] ?? "?", w[kCGWindowName as String] ?? "")
}

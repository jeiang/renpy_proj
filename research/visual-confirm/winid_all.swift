// usage: winid_all PID...  prints "id layer onscreen w h name" for every window owned by those PIDs (debug for windows winid misses).
import CoreGraphics
import Foundation
let pids = Set(CommandLine.arguments.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    if let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p) {
        let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        print(w[kCGWindowNumber as String] ?? "?", w[kCGWindowLayer as String] ?? "?", w[kCGWindowIsOnscreen as String] ?? "no", b["Width"] ?? "?", b["Height"] ?? "?", w[kCGWindowName as String] ?? "")
    }
}

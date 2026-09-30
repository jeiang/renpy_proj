// wintool list PID...   one line per window of those PIDs: "id layer onscreen width height name"
// wintool info ID       "pid P onscreen 0|1 front N visible F covered_by PID:NAME,..." for one window: its place in the
//                       on-screen stack (N windows in front of it) and the share F (0..1) of its area that no window in
//                       front of it hides (a 40x40 sample grid). Windows of the same process and fully transparent ones do not count.
// Read-only: it lists windows so the gate can capture only windows it launched (screencapture -l <id>).
import CoreGraphics
import Foundation

let args = Array(CommandLine.arguments.dropFirst())

func rect(_ w: [String: Any]) -> CGRect {
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    return CGRect(x: (b["X"] as? Double) ?? 0, y: (b["Y"] as? Double) ?? 0,
                  width: (b["Width"] as? Double) ?? 0, height: (b["Height"] as? Double) ?? 0)
}

if args.first == "info", args.count > 1, let id = Int(args[1]) {
    let all = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
    let shown = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []   // front to back
    guard let t = all.first(where: { ($0[kCGWindowNumber as String] as? Int) == id }) else { print("gone"); exit(2) }
    let pid = t[kCGWindowOwnerPID as String] as? Int32 ?? 0
    let onscreen = shown.contains { ($0[kCGWindowNumber as String] as? Int) == id }
    var front: [[String: Any]] = []
    for w in shown {
        if (w[kCGWindowNumber as String] as? Int) == id { break }
        if (w[kCGWindowOwnerPID as String] as? Int32) == pid { continue }
        if ((w[kCGWindowAlpha as String] as? Double) ?? 1) < 0.05 { continue }
        front.append(w)
    }
    let r = rect(t)
    var seen = 0, total = 0
    var by = Set<String>()
    if onscreen && r.width > 0 && r.height > 0 {
        for i in 0..<40 {
            for j in 0..<40 {
                let p = CGPoint(x: r.minX + r.width * (Double(i) + 0.5) / 40, y: r.minY + r.height * (Double(j) + 0.5) / 40)
                total += 1
                if let c = front.first(where: { rect($0).contains(p) }) {
                    by.insert("\(c[kCGWindowOwnerPID as String] ?? 0):\(c[kCGWindowOwnerName as String] ?? "?")")
                } else { seen += 1 }
            }
        }
    }
    print("pid", pid, "onscreen", onscreen ? 1 : 0, "front", front.count, "visible",
          total > 0 ? String(format: "%.3f", Double(seen) / Double(total)) : "0.000", "covered_by", by.sorted().joined(separator: ","))
    exit(0)
}

let pids = Set(args.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    guard let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p) else { continue }
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    print(w[kCGWindowNumber as String] ?? "?", w[kCGWindowLayer as String] ?? "?",
          w[kCGWindowIsOnscreen as String] ?? "no", b["Width"] ?? "?", b["Height"] ?? "?", w[kCGWindowName as String] ?? "")
}

// usage: swift winid.swift PID...   prints CGWindowIDs (on-screen, layer 0) owned by those PIDs, one per line.
import CoreGraphics
import Foundation
let pids = Set(CommandLine.arguments.dropFirst().compactMap { Int32($0) })
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for w in list {
    if let p = w[kCGWindowOwnerPID as String] as? Int32, pids.contains(p),
       (w[kCGWindowLayer as String] as? Int) == 0, let n = w[kCGWindowNumber as String] as? Int { print(n) }
}

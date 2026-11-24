// winwait <pid> <timeout s>: waits until the process has a normal window on screen at least 700 × 450 points (a splash
// screen is smaller), then prints the CLOCK_UPTIME_RAW time in ns (Python: time.clock_gettime_ns(time.CLOCK_UPTIME_RAW)).
// Exit 1 on timeout. Reads only window owners and bounds, which needs no Screen Recording permission.
// Built by scripts/bench.sh (--gui, --compare):  swiftc -O winwait.swift -o winwait
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 3, let pid = Int(args[1]), let timeout = Double(args[2]) else {
    FileHandle.standardError.write("usage: winwait <pid> <timeout s>\n".data(using: .utf8)!)
    exit(2)
}
let end = Date().addingTimeInterval(timeout)
while Date() < end {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    let shown = list.contains { w in
        guard (w[kCGWindowOwnerPID as String] as? Int) == pid, (w[kCGWindowLayer as String] as? Int) == 0,
              let b = w[kCGWindowBounds as String] as? [String: Double] else { return false }
        return (b["Width"] ?? 0) >= 700 && (b["Height"] ?? 0) >= 450
    }
    if shown {
        print(clock_gettime_nsec_np(CLOCK_UPTIME_RAW))
        exit(0)
    }
    usleep(5_000)
}
exit(1)

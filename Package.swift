// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "CopyPaster",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "CopyPaster",
            path: "Sources/CopyPaster"
        )
    ]
)

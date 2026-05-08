// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "sckit_capture",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "sckit_capture", targets: ["sckit_capture"])
    ],
    targets: [
        .executableTarget(
            name: "sckit_capture",
            path: "Sources/sckit_capture"
        )
    ]
)

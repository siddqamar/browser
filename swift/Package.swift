// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Browser",
    platforms: [
        .macOS(.v13)
    ],
    targets: [
        .systemLibrary(
            name: "CBrowser",
            path: "Sources/CBrowser"
        ),
        .executableTarget(
            name: "Browser",
            dependencies: ["CBrowser"],
            linkerSettings: [
                .unsafeFlags([
                    "-L", "/Users/luna/code/imlunahey/browser/target/debug",
                    "-lbrowser_ffi",
                ]),
                .linkedFramework("AppKit"),
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
    ]
)

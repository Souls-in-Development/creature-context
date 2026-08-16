// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "CreatureContextFoundation",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "CreatureContextFoundation", targets: ["CreatureContextFoundation"]),
        .executable(name: "AtlasMenuBar", targets: ["AtlasMenuBar"]),
    ],
    targets: [
        .target(name: "CreatureContextFoundation"),
        .executableTarget(name: "AtlasMenuBar"),
        .testTarget(name: "CreatureContextFoundationTests", dependencies: ["CreatureContextFoundation"]),
    ]
)

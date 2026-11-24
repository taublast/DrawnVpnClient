// The connection system extension's entry point (macOS): the system starts it as a process of its
// own; NetworkExtension then creates the packet tunnel provider (ios/connection/Provider.swift, the
// same as on iOS) named in the extension's Info.plist.
import Foundation
import NetworkExtension

autoreleasepool {
    NEProvider.startSystemExtensionMode()
}
dispatchMain()

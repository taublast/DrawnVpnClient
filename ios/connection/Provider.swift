// The connection extension (a packet tunnel provider): the system's tunnel with every route, and the
// engine (Xray-core, ios/engine) reading its utun fd, as ConnectionService does on Android. The
// extension's own connections (Xray's to the server) do not go through its tunnel.
import Darwin
import NetworkExtension

/// Answers 204 with no body: a cheap check that traffic really passes the server.
private let probeURL = "https://www.gstatic.com/generate_204"
private let probeTimeoutMs: Int32 = 10_000

final class Provider: NEPacketTunnelProvider {
    private let queue = DispatchQueue(label: "connection")

    override func startTunnel(options: [String: NSObject]?, completionHandler: @escaping (Error?) -> Void) {
        // From the app's start call, else (the system restarting it) the saved configuration.
        let saved = (protocolConfiguration as? NETunnelProviderProtocol)?.providerConfiguration?["config"] as? String
        guard let config = (options?["config"] as? String) ?? saved else {
            completionHandler(failure("No server configuration"))
            return
        }
        setTunnelNetworkSettings(Self.settings()) { error in
            if let error {
                completionHandler(error)
                return
            }
            self.queue.async {
                guard let fd = Self.tunnelFd() else {
                    completionHandler(failure("No tunnel interface"))
                    return
                }
                if let message = config.withCString({ EngineStart(UnsafeMutablePointer(mutating: $0), fd) }) {
                    let text = String(cString: message)
                    EngineFree(message)
                    completionHandler(failure(text))
                    return
                }
                completionHandler(nil)
            }
        }
    }

    override func stopTunnel(with reason: NEProviderStopReason, completionHandler: @escaping () -> Void) {
        queue.async {
            EngineStop()
            completionHandler()
        }
    }

    /// "test": a request through the server, answered with its milliseconds, or "" for no answer.
    /// "version": the engine's version.
    override func handleAppMessage(_ messageData: Data, completionHandler: ((Data?) -> Void)?) {
        let message = String(decoding: messageData, as: UTF8.self)
        queue.async {
            var answer = ""
            switch message {
            case "test":
                let ms = probeURL.withCString { EngineMeasure(UnsafeMutablePointer(mutating: $0), probeTimeoutMs) }
                answer = ms >= 0 ? String(ms) : ""
            case "version":
                if let version = EngineVersion() {
                    answer = String(cString: version)
                    EngineFree(version)
                }
            default:
                break
            }
            completionHandler?(Data(answer.utf8))
        }
    }

    /// As on Android: 10.10.14.1/30 and fd66:7665:7373::1/126 with the default routes, DNS 1.1.1.1,
    /// MTU 1500.
    private static func settings() -> NEPacketTunnelNetworkSettings {
        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "10.10.14.2")
        let ipv4 = NEIPv4Settings(addresses: ["10.10.14.1"], subnetMasks: ["255.255.255.252"])
        ipv4.includedRoutes = [NEIPv4Route.default()]
        settings.ipv4Settings = ipv4
        let ipv6 = NEIPv6Settings(addresses: ["fd66:7665:7373::1"], networkPrefixLengths: [126])
        ipv6.includedRoutes = [NEIPv6Route.default()]
        settings.ipv6Settings = ipv6
        let dns = NEDNSSettings(servers: ["1.1.1.1"])
        dns.matchDomains = [""]
        settings.dnsSettings = dns
        settings.mtu = 1500
        return settings
    }

    /// The utun fd NetworkExtension opened for this tunnel (Xray-core's proxy/tun README, iOS).
    private static func tunnelFd() -> Int32? {
        var name = [CChar](repeating: 0, count: Int(IFNAMSIZ))
        let prefix = Array("utun".utf8CString.dropLast())
        for fd: Int32 in 0...1024 {
            var length = socklen_t(name.count)
            // SYSPROTO_CONTROL, UTUN_OPT_IFNAME
            if getsockopt(fd, 2, 2, &name, &length) == 0, name.starts(with: prefix) {
                return fd
            }
        }
        return nil
    }
}

private func failure(_ text: String) -> NSError {
    NSError(domain: "connection", code: 1, userInfo: [NSLocalizedDescriptionKey: text])
}

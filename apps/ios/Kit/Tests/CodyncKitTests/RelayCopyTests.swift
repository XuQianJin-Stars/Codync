import Foundation
import Testing
@testable import CodyncKit

struct RelayCopyTests {
    @Test func officialCloudsKeepTheCloudflareName() {
        #expect(RelayCopy.host(of: "https://api.codync.dev") == nil)
        #expect(RelayCopy.host(of: "https://dev-api.codync.dev/") == nil)
        #expect(RelayCopy.host(of: nil as String?) == nil)
        #expect(RelayCopy.connected(cloud: URL(string: "https://api.codync.dev")) == "Connected through Cloudflare")
        #expect(RelayCopy.routeTitle(.automatic, cloud: nil) == "Auto: Wi-Fi or Tailscale, then Cloudflare")
    }

    @Test func anotherHostIsNamedInTheSentence() {
        let cloud = URL(string: "https://relay.example.com")
        #expect(RelayCopy.host(of: cloud) == "relay.example.com")
        #expect(RelayCopy.host(of: "http://127.0.0.1:8788") == "127.0.0.1:8788")
        #expect(RelayCopy.connected(cloud: cloud) == "Connected through Private relay (relay.example.com)")
        #expect(RelayCopy.through(cloud: cloud) == "Through Private relay (relay.example.com)")
        #expect(RelayCopy.routeTitle(.cloudflareFirst, cloud: cloud) == "Private relay (relay.example.com), then Wi-Fi or Tailscale")
        #expect(RelayCopy.detailSuffix(route: .relay, cloud: cloud) == " · Private relay (relay.example.com)")
        #expect(RelayCopy.pairing(cloud: cloud).contains("Private relay (relay.example.com)"))
    }
}

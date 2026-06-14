package fetchurl

import (
	"fmt"
	"net"
	"net/url"
	"strings"
)

// allowPrivateHosts is test-only; production fetches block private/loopback targets.
var allowPrivateHosts bool

// SetAllowPrivateHostsForTest permits loopback and private IPs during tests.
func SetAllowPrivateHostsForTest(v bool) {
	allowPrivateHosts = v
}

func parsePublicURL(raw string) (*url.URL, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("empty URL")
	}
	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("invalid URL: %w", err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return nil, fmt.Errorf("only http and https URLs are supported")
	}
	if strings.TrimSpace(u.Host) == "" {
		return nil, fmt.Errorf("URL missing host")
	}
	host := strings.ToLower(u.Hostname())
	if !allowPrivateHosts && (host == "localhost" || strings.HasSuffix(host, ".localhost")) {
		return nil, fmt.Errorf("localhost URLs are not allowed")
	}
	if ip := net.ParseIP(host); ip != nil {
		if isBlockedIP(ip) {
			return nil, fmt.Errorf("private or reserved IP addresses are not allowed")
		}
		return u, nil
	}
	ips, err := net.LookupIP(host)
	if err != nil {
		return nil, fmt.Errorf("resolve host: %w", err)
	}
	if len(ips) == 0 {
		return nil, fmt.Errorf("host resolved to no addresses")
	}
	for _, ip := range ips {
		if isBlockedIP(ip) {
			return nil, fmt.Errorf("host resolves to private or reserved IP")
		}
	}
	return u, nil
}

func isBlockedIP(ip net.IP) bool {
	if allowPrivateHosts {
		return false
	}
	if ip.IsLoopback() || ip.IsPrivate() || ip.IsLinkLocalUnicast() || ip.IsLinkLocalMulticast() || ip.IsUnspecified() {
		return true
	}
	if ip4 := ip.To4(); ip4 != nil {
		switch {
		case ip4[0] == 0:
			return true
		case ip4[0] == 169 && ip4[1] == 254:
			return true
		case ip4[0] == 127:
			return true
		}
	}
	return false
}

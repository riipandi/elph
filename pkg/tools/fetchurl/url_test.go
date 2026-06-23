package fetchurl

import (
	"net"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestIsBlockedIP_RespectsAllowPrivateHosts(t *testing.T) {
	allowPrivateHosts = true
	defer func() { allowPrivateHosts = false }()

	require.False(t, isBlockedIP(net.ParseIP("127.0.0.1")))
}

func TestIsBlockedIP_Loopback(t *testing.T) {
	require.True(t, isBlockedIP(net.ParseIP("127.0.0.1")))
	require.True(t, isBlockedIP(net.ParseIP("::1")))
}

func TestIsBlockedIP_Private(t *testing.T) {
	require.True(t, isBlockedIP(net.ParseIP("10.0.0.1")))
	require.True(t, isBlockedIP(net.ParseIP("172.16.0.1")))
	require.True(t, isBlockedIP(net.ParseIP("192.168.1.1")))
}

func TestIsBlockedIP_LinkLocal(t *testing.T) {
	require.True(t, isBlockedIP(net.ParseIP("169.254.1.1")))
	require.True(t, isBlockedIP(net.ParseIP("fe80::1")))
}

func TestIsBlockedIP_Unspecified(t *testing.T) {
	require.True(t, isBlockedIP(net.ParseIP("0.0.0.0")))
	require.True(t, isBlockedIP(net.ParseIP("::")))
}

func TestIsBlockedIP_PublicIPAllowed(t *testing.T) {
	require.False(t, isBlockedIP(net.ParseIP("8.8.8.8")))
	require.False(t, isBlockedIP(net.ParseIP("1.1.1.1")))
	require.False(t, isBlockedIP(net.ParseIP("2001:4860:4860::8888")))
}

func TestIsBlockedIP_ZeroPrefix(t *testing.T) {
	// 0.0.0.1 should be blocked by the ip.IsUnspecified() check plus the explicit ip4[0]==0
	require.True(t, isBlockedIP(net.ParseIP("0.0.0.1")))
}

// nolint:unparam
func TestParsePublicURL_Empty(t *testing.T) {
	_, err := parsePublicURL("")
	require.Error(t, err)
	require.Contains(t, err.Error(), "empty")
}

func TestParsePublicURL_InvalidURL(t *testing.T) {
	_, err := parsePublicURL("://invalid")
	require.Error(t, err)
}

func TestParsePublicURL_UnsupportedScheme(t *testing.T) {
	_, err := parsePublicURL("ftp://example.com")
	require.Error(t, err)
	require.Contains(t, err.Error(), "only http and https")
}

func TestParsePublicURL_MissingHost(t *testing.T) {
	_, err := parsePublicURL("http://")
	require.Error(t, err)
	require.Contains(t, err.Error(), "missing host")
}

func TestParsePublicURL_MissingHostWhitespace(t *testing.T) {
	_, err := parsePublicURL("http://  ")
	require.Error(t, err)
	require.Contains(t, err.Error(), "missing host")
}

func TestParsePublicURL_RejectsLocalhost(t *testing.T) {
	_, err := parsePublicURL("http://localhost/path")
	require.Error(t, err)
	require.Contains(t, err.Error(), "localhost")
}

func TestParsePublicURL_RejectsSubdomainLocalhost(t *testing.T) {
	_, err := parsePublicURL("http://foo.localhost/path")
	require.Error(t, err)
	require.Contains(t, err.Error(), "localhost")
}

func TestParsePublicURL_RejectsPrivateIP(t *testing.T) {
	_, err := parsePublicURL("http://192.168.1.1/admin")
	require.Error(t, err)
	require.Contains(t, err.Error(), "private")
}

func TestParsePublicURL_ValidPublicURL(t *testing.T) {
	u, err := parsePublicURL("https://example.com/path?q=1")
	require.NoError(t, err)
	require.Equal(t, "example.com", u.Host)
	require.Equal(t, "/path", u.Path)
	require.Equal(t, "q=1", u.RawQuery)
}

func TestParsePublicURL_TrimsWhitespace(t *testing.T) {
	u, err := parsePublicURL("  https://example.com  ")
	require.NoError(t, err)
	require.Equal(t, "example.com", u.Host)
}

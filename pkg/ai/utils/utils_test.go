package utils

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	"resty.dev/v3"
)

func TestMergeStringMaps_BothEmpty(t *testing.T) {
	require.Nil(t, MergeStringMaps(nil, nil))
	require.Nil(t, MergeStringMaps(map[string]string{}, map[string]string{}))
}

func TestMergeStringMaps_BaseOnly(t *testing.T) {
	got := MergeStringMaps(map[string]string{"a": "1"}, nil)
	require.Equal(t, map[string]string{"a": "1"}, got)
}

func TestMergeStringMaps_Override(t *testing.T) {
	got := MergeStringMaps(map[string]string{"a": "1", "b": "2"}, map[string]string{"b": "3", "c": "4"})
	require.Equal(t, map[string]string{"a": "1", "b": "3", "c": "4"}, got)
}

func TestNewHTTPClient(t *testing.T) {
	c := NewHTTPClient()
	require.NotNil(t, c)
}

func TestNewHTTPClientWithTimeout(t *testing.T) {
	c := NewHTTPClientWithTimeout(5 * time.Second)
	require.NotNil(t, c)
}

func TestNewStreamingHTTPClient(t *testing.T) {
	c := NewStreamingHTTPClient()
	require.NotNil(t, c)
}

func TestTrimBody_Short(t *testing.T) {
	require.Equal(t, "hello", trimBody([]byte("hello")))
}

func TestTrimBody_TruncatesLong(t *testing.T) {
	s := string(make([]byte, 300))
	got := trimBody([]byte(s))
	require.Len(t, got, 240+3) // 240 chars + "..."
	require.Contains(t, got, "...")
}

func TestTrimBody_StripsWhitespace(t *testing.T) {
	got := trimBody([]byte("  hello  "))
	require.Equal(t, "hello", got)
}

func TestPostJSON_Success(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "POST", r.Method)
		require.Equal(t, "application/json", r.Header.Get("Content-Type"))

		var reqBody map[string]string
		require.NoError(t, json.NewDecoder(r.Body).Decode(&reqBody))
		require.Equal(t, "world", reqBody["hello"])

		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	}))
	defer srv.Close()

	var out struct {
		Status string `json:"status"`
	}
	err := PostJSON(context.Background(), resty.New(), srv.URL, map[string]string{"X-Custom": "val"}, map[string]string{"hello": "world"}, &out)
	require.NoError(t, err)
	require.Equal(t, "ok", out.Status)
}

func TestPostJSON_NilClient(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer srv.Close()

	var out struct {
		Ok bool `json:"ok"`
	}
	err := PostJSON(context.Background(), nil, srv.URL, nil, nil, &out)
	require.NoError(t, err)
	require.True(t, out.Ok)
}

func TestPostJSON_ErrorStatus(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		_, _ = w.Write([]byte(`bad request`))
	}))
	defer srv.Close()

	err := PostJSON(context.Background(), resty.New(), srv.URL, nil, nil, nil)
	require.Error(t, err)
	require.Contains(t, err.Error(), "400")
}

func TestGetJSON_Success(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "GET", r.Method)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"value":42}`))
	}))
	defer srv.Close()

	var out struct {
		Value int `json:"value"`
	}
	err := GetJSON(context.Background(), resty.New(), srv.URL, &out)
	require.NoError(t, err)
	require.Equal(t, 42, out.Value)
}

func TestGetJSON_NilClient(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer srv.Close()

	var out struct {
		Ok bool `json:"ok"`
	}
	err := GetJSON(context.Background(), nil, srv.URL, &out)
	require.NoError(t, err)
	require.True(t, out.Ok)
}

func TestGetJSONWithHeaders(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "test-value", r.Header.Get("X-Test"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	}))
	defer srv.Close()

	var out struct {
		Status string `json:"status"`
	}
	err := GetJSONWithHeaders(context.Background(), resty.New(), srv.URL, map[string]string{"X-Test": "test-value"}, &out)
	require.NoError(t, err)
	require.Equal(t, "ok", out.Status)
}

func TestUpstreamHTTPError(t *testing.T) {
	err := upstreamHTTPError(500, []byte("internal error"))
	require.Error(t, err)
}

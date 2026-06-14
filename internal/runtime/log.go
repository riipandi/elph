package runtime

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/projectdir"
	"go.jetify.com/typeid/v2"
)

const (
	defaultLogTailBytes = 24 * 1024
	eventsLogName       = "events.jsonl"
	requestsLogName     = "requests.jsonl"
)

func ensureSessionLogDir(workDir string, sessionID typeid.TypeID) (string, error) {
	if err := projectdir.EnsureRoot(workDir); err != nil {
		return "", err
	}
	dir := projectdir.SessionDir(workDir, sessionID.String())
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}

func openSessionLogFile(workDir, filename string, sessionID typeid.TypeID) (string, error) {
	dir, err := ensureSessionLogDir(workDir, sessionID)
	if err != nil {
		return "", err
	}
	path := filepath.Join(dir, filename)
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return "", err
	}
	defer f.Close()
	return path, nil
}

// OpenLog opens or creates the session events log for workDir and sessionID.
func OpenLog(workDir string, sessionID typeid.TypeID) (string, error) {
	return openSessionLogFile(workDir, eventsLogName, sessionID)
}

// OpenRequestsLog opens or creates the provider request trace for a session.
func OpenRequestsLog(workDir string, sessionID typeid.TypeID) (string, error) {
	return openSessionLogFile(workDir, requestsLogName, sessionID)
}

// AppendLog appends a structured JSON line to the session log using slog.
func AppendLog(path, kind, text string) error {
	if path == "" {
		return nil
	}

	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()

	logger := slog.New(slog.NewJSONHandler(f, &slog.HandlerOptions{
		Level: slog.LevelInfo,
	}))
	logger.Info(strings.TrimRight(text, "\n"), slog.String("kind", kind))
	return nil
}

// RequestsLogPath returns the path for a session's provider/tool request log.
func RequestsLogPath(workDir string, sessionID typeid.TypeID) string {
	return filepath.Join(projectdir.SessionDir(workDir, sessionID.String()), requestsLogName)
}

type logRecord struct {
	Time string `json:"time"`
	Kind string `json:"kind"`
	Msg  string `json:"msg"`
}

// FilterLogByKind returns log lines tagged with the given kind.
func FilterLogByKind(path, kind string, maxBytes int) (string, error) {
	raw, err := readRawLogTail(path, maxBytes)
	if err != nil {
		return "", err
	}

	var lines []string
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		rec, ok := parseLogRecord(line)
		if !ok || rec.Kind != kind {
			continue
		}
		lines = append(lines, formatLogRecord(rec))
	}
	return strings.Join(lines, "\n"), nil
}

func formatLogRecord(rec logRecord) string {
	ts := rec.Time
	if ts == "" {
		ts = time.Now().UTC().Format(time.RFC3339)
	}
	return fmt.Sprintf("%s [%s] %s", ts, rec.Kind, rec.Msg)
}

// ReadLogTail returns up to maxBytes from the end of the log file, formatting JSONL
// records for display when possible.
func ReadLogTail(path string, maxBytes int) (string, error) {
	raw, err := readRawLogTail(path, maxBytes)
	if err != nil {
		return "", err
	}
	return formatLogTail(raw), nil
}

func readRawLogTail(path string, maxBytes int) (string, error) {
	if path == "" {
		return "", fmt.Errorf("log path is empty")
	}
	if maxBytes <= 0 {
		maxBytes = defaultLogTailBytes
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	if len(data) == 0 {
		return "", nil
	}
	if len(data) > maxBytes {
		data = data[len(data)-maxBytes:]
		if idx := strings.IndexByte(string(data), '\n'); idx >= 0 && idx < len(data)-1 {
			data = data[idx+1:]
		}
	}
	return string(data), nil
}

func parseLogRecord(line string) (logRecord, bool) {
	var rec logRecord
	if err := json.Unmarshal([]byte(line), &rec); err != nil {
		return logRecord{}, false
	}
	if rec.Msg == "" {
		var generic map[string]any
		if err := json.Unmarshal([]byte(line), &generic); err != nil {
			return logRecord{}, false
		}
		if v, ok := generic["msg"].(string); ok {
			rec.Msg = v
		}
		if v, ok := generic["kind"].(string); ok {
			rec.Kind = v
		}
		if v, ok := generic["time"].(string); ok {
			rec.Time = v
		}
	}
	if rec.Msg == "" {
		return logRecord{}, false
	}
	return rec, true
}

func formatLogTail(raw string) string {
	lines := strings.Split(strings.TrimRight(raw, "\n"), "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		rec, ok := parseLogRecord(line)
		if !ok {
			out = append(out, line)
			continue
		}
		out = append(out, formatLogRecord(rec))
	}
	return strings.Join(out, "\n")
}

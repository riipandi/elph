package runtime

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"go.jetify.com/typeid/v2"
)

const defaultLogTailBytes = 24 * 1024

func sessionLogDir(workDir string) (string, error) {
	dir := filepath.Join(workDir, ".elph", "logs")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}

func openSessionLogFile(workDir, filename string) (string, error) {
	dir, err := sessionLogDir(workDir)
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

// OpenLog opens or creates the session log file for workDir and sessionID.
func OpenLog(workDir string, sessionID typeid.TypeID) (string, error) {
	return openSessionLogFile(workDir, sessionID.String()+".log")
}

// OpenRequestsLog opens or creates the provider request trace for a session.
func OpenRequestsLog(workDir string, sessionID typeid.TypeID) (string, error) {
	return openSessionLogFile(workDir, sessionID.String()+".requests.log")
}

// AppendLog appends a timestamped line to the session log.
func AppendLog(path, kind, text string) error {
	if path == "" {
		return nil
	}

	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()

	ts := time.Now().UTC().Format(time.RFC3339)
	body := strings.TrimRight(text, "\n")
	_, err = fmt.Fprintf(f, "%s [%s] %s\n", ts, kind, body)
	return err
}

// RequestsLogPath returns the path for a session's provider/tool request log.
func RequestsLogPath(workDir string, sessionID typeid.TypeID) string {
	return filepath.Join(workDir, ".elph", "logs", sessionID.String()+".requests.log")
}

// FilterLogByKind returns log lines tagged with the given kind.
func FilterLogByKind(path, kind string, maxBytes int) (string, error) {
	content, err := ReadLogTail(path, maxBytes)
	if err != nil {
		return "", err
	}

	marker := fmt.Sprintf(" [%s] ", kind)
	var lines []string
	for _, line := range strings.Split(content, "\n") {
		if strings.Contains(line, marker) {
			lines = append(lines, line)
		}
	}
	return strings.Join(lines, "\n"), nil
}

// ReadLogTail returns up to maxBytes from the end of the log file.
func ReadLogTail(path string, maxBytes int) (string, error) {
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
	if len(data) <= maxBytes {
		return string(data), nil
	}

	truncated := data[len(data)-maxBytes:]
	if idx := strings.IndexByte(string(truncated), '\n'); idx >= 0 && idx < len(truncated)-1 {
		truncated = truncated[idx+1:]
	}
	return string(truncated), nil
}

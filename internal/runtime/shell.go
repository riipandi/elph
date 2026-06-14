package runtime

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
)

const (
	defaultMaxShellLines = 2000
	defaultMaxShellBytes = 50 * 1024
)

// ShellResult holds the outcome of a user-initiated shell command.
type ShellResult struct {
	Output    string
	ExitCode  int
	Err       error
	Cancelled bool
}

// RunShell executes command via bash -c in workDir without cancellation.
func RunShell(workDir, command string) ShellResult {
	return RunShellContext(context.Background(), workDir, command, nil)
}

// RunShellContext executes a shell command and streams stdout/stderr chunks to onChunk.
// Cancel ctx to terminate the process; partial output is preserved in ShellResult.
func RunShellContext(ctx context.Context, workDir, command string, onChunk func(string)) ShellResult {
	cmd := exec.CommandContext(ctx, "bash", "-c", command)
	cmd.Dir = workDir
	configureShellProcess(cmd)

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return ShellResult{Err: err, ExitCode: -1}
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return ShellResult{Err: err, ExitCode: -1}
	}

	if err := cmd.Start(); err != nil {
		cancelled := errors.Is(err, context.Canceled) || errors.Is(ctx.Err(), context.Canceled)
		return ShellResult{Err: err, ExitCode: -1, Cancelled: cancelled}
	}

	pgid := shellProcessGroupID(cmd.Process.Pid)
	killOnCancel(ctx, pgid)

	var (
		mu     sync.Mutex
		output strings.Builder
	)
	appendChunk := func(chunk string) {
		if chunk == "" {
			return
		}
		mu.Lock()
		output.WriteString(chunk)
		mu.Unlock()
		if onChunk != nil {
			onChunk(chunk)
		}
	}

	copyOut := func(r io.Reader, wg *sync.WaitGroup) {
		defer wg.Done()
		buf := make([]byte, 4096)
		for {
			n, readErr := r.Read(buf)
			if n > 0 {
				appendChunk(string(buf[:n]))
			}
			if readErr != nil {
				return
			}
		}
	}

	var wg sync.WaitGroup
	wg.Add(2)
	go copyOut(stdout, &wg)
	go copyOut(stderr, &wg)

	waitErr := cmd.Wait()
	wg.Wait()

	cancelled := ctx.Err() != nil
	raw := strings.TrimRight(output.String(), "\n")
	truncated := truncateShellOutput(raw)

	result := ShellResult{
		Output:    truncated,
		Cancelled: cancelled,
	}

	if waitErr != nil {
		if cancelled {
			return result
		}
		exitErr, ok := waitErr.(*exec.ExitError)
		if ok {
			result.ExitCode = exitErr.ExitCode()
			return result
		}
		result.Err = waitErr
		result.ExitCode = -1
		return result
	}

	return result
}

// FormatShellContext returns Pi-style text sent to the agent for ! commands.
func FormatShellContext(command, output string, exitCode int) string {
	var b strings.Builder
	fmt.Fprintf(&b, "Ran `%s`\n", command)
	if output != "" {
		b.WriteString("```\n")
		b.WriteString(output)
		b.WriteString("\n```")
	} else {
		b.WriteString("(no output)")
	}
	if exitCode != 0 {
		fmt.Fprintf(&b, "\n\n(exit %d)", exitCode)
	}
	return b.String()
}

// SplitShellExitSuffix separates trailing "(exit N)" metadata from bash tool output.
func SplitShellExitSuffix(output string) (body string, exitCode int) {
	trimmed := strings.TrimRight(output, "\n")
	if trimmed == "" {
		return output, 0
	}
	const prefix = "(exit "
	if strings.HasPrefix(trimmed, prefix) && strings.HasSuffix(trimmed, ")") {
		inner := strings.TrimSuffix(strings.TrimPrefix(trimmed, prefix), ")")
		if code, err := strconv.Atoi(inner); err == nil {
			return "", code
		}
	}
	const marker = "\n\n(exit "
	if idx := strings.LastIndex(trimmed, marker); idx >= 0 {
		suffix := trimmed[idx+len(marker):]
		if strings.HasSuffix(suffix, ")") {
			inner := strings.TrimSuffix(suffix, ")")
			if code, err := strconv.Atoi(inner); err == nil {
				return trimmed[:idx], code
			}
		}
	}
	return output, 0
}

// FormatBashToolDetailBody formats agent Bash tool output for the detail box.
// preferStream keeps streamed UI text when the tool finishes.
func FormatBashToolDetailBody(result ToolResult, preferStream string) string {
	streamed := strings.TrimRight(preferStream, "\n")
	body, exitCode := SplitShellExitSuffix(result.Output)
	body = strings.TrimRight(body, "\n")
	if streamed != "" {
		body = streamed
	}
	if result.Cancelled {
		return FormatShellDetailBody(body, 0, nil, true)
	}
	if result.Err != nil {
		return FormatShellDetailBody(body, 0, result.Err, false)
	}
	return FormatShellDetailBody(body, exitCode, nil, false)
}

// FormatShellDetailBody returns collapsible detail text for shell output (without the command line).
func FormatShellDetailBody(output string, exitCode int, runErr error, cancelled bool) string {
	if cancelled {
		var b strings.Builder
		if output != "" {
			b.WriteString(output)
		}
		if b.Len() > 0 {
			b.WriteByte('\n')
		}
		b.WriteString("(cancelled)")
		return b.String()
	}
	if runErr != nil {
		var b strings.Builder
		if output != "" {
			b.WriteString(output)
			b.WriteByte('\n')
		}
		b.WriteString(runErr.Error())
		return b.String()
	}
	var b strings.Builder
	if output != "" {
		b.WriteString(output)
	}
	if exitCode != 0 {
		if b.Len() > 0 {
			b.WriteByte('\n')
		}
		fmt.Fprintf(&b, "(exit %d)", exitCode)
	}
	return b.String()
}

// FormatShellDisplay returns UI text for bash execution in the chat stream.
func FormatShellDisplay(command, output string, exitCode int, runErr error, cancelled bool) string {
	var b strings.Builder
	fmt.Fprintf(&b, "$ %s", command)
	if cancelled {
		if output != "" {
			b.WriteString("\n")
			b.WriteString(output)
		}
		b.WriteString("\n(cancelled)")
		return b.String()
	}
	if runErr != nil {
		if output != "" {
			b.WriteString("\n")
			b.WriteString(output)
		}
		b.WriteString("\n")
		b.WriteString(runErr.Error())
		return b.String()
	}
	if output != "" {
		b.WriteString("\n")
		b.WriteString(output)
	}
	if exitCode != 0 {
		fmt.Fprintf(&b, "\n(exit %d)", exitCode)
	}
	return b.String()
}

func truncateShellOutput(s string) string {
	if s == "" {
		return s
	}
	lines := strings.Split(s, "\n")
	truncated := false

	if len(lines) > defaultMaxShellLines {
		lines = lines[len(lines)-defaultMaxShellLines:]
		truncated = true
	}

	out := strings.Join(lines, "\n")
	for len(out) > defaultMaxShellBytes && len(lines) > 1 {
		lines = lines[1:]
		out = strings.Join(lines, "\n")
		truncated = true
	}
	if len(out) > defaultMaxShellBytes {
		out = out[len(out)-defaultMaxShellBytes:]
		truncated = true
	}
	if truncated {
		out = fmt.Sprintf("... (output truncated)\n%s", out)
	}
	return out
}

func configureShellProcess(cmd *exec.Cmd) {
	if runtime.GOOS == "windows" {
		return
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func shellProcessGroupID(pid int) int {
	if pid <= 0 {
		return 0
	}
	pgid, err := syscall.Getpgid(pid)
	if err != nil {
		return 0
	}
	return pgid
}

// killOnCancel terminates the captured process group when ctx is canceled.
// pgid must be recorded immediately after cmd.Start to avoid PID reuse races
// when parallel tests run shell commands.
func killOnCancel(ctx context.Context, pgid int) {
	if pgid <= 0 {
		return
	}
	go func() {
		<-ctx.Done()
		_ = syscall.Kill(-pgid, syscall.SIGKILL)
	}()
}

// SanitizeStreamChunk normalizes streamed shell bytes for display.
// Prefer ApplyStreamChunk when accumulating output across chunks.
func SanitizeStreamChunk(chunk string) string {
	return strings.NewReplacer("\r\n", "\n", "\r", "\n").Replace(chunk)
}

// ApplyStreamChunk appends a shell output chunk to acc, honoring carriage
// returns used by tools like ping to overwrite the current line.
func ApplyStreamChunk(acc, chunk string) string {
	for i := 0; i < len(chunk); {
		switch chunk[i] {
		case '\r':
			if i+1 < len(chunk) && chunk[i+1] == '\n' {
				acc += "\n"
				i += 2
				continue
			}
			if idx := strings.LastIndex(acc, "\n"); idx >= 0 {
				acc = acc[:idx+1]
			} else {
				acc = ""
			}
			i++
		case '\n':
			acc += "\n"
			i++
		default:
			j := i
			for j < len(chunk) && chunk[j] != '\r' && chunk[j] != '\n' {
				j++
			}
			acc += chunk[i:j]
			i = j
		}
	}
	return acc
}

// TrimStreamOutput trims trailing newlines from completed stream output.
// Do not call this after every streamed chunk; chunk boundaries often end on \n.
func TrimStreamOutput(s string) string {
	return strings.TrimRight(s, "\n")
}

package shell

import (
	"context"
	"errors"
	"fmt"
	"os/exec"
	gort "runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"

	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/xpty"
	"github.com/riipandi/elph/internal/runtime/toolresult"
)

// ptySema limits concurrent PTY allocations to avoid kernel resource exhaustion
// (ENXIO) under parallel load.
var ptySema = make(chan struct{}, 16)

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

// RunShellContext executes a shell command via PTY and streams output chunks to onChunk.
// Cancel ctx to terminate the process; partial output is preserved in ShellResult.
// PTY output may contain ANSI escape codes; caller can strip them with ansi.Strip
// on the resulting ShellResult.Output.
func RunShellContext(ctx context.Context, workDir, command string, onChunk func(string)) ShellResult {
	ptySema <- struct{}{}
	defer func() { <-ptySema }()

	pty, err := xpty.NewPty(80, 24)
	if err != nil {
		return ShellResult{Err: fmt.Errorf("create pty: %w", err), ExitCode: -1}
	}
	defer pty.Close()

	cmd := exec.Command("bash", "-c", command)
	cmd.Dir = workDir
	configureShellProcess(cmd)

	if err := pty.Start(cmd); err != nil {
		cancelled := errors.Is(err, context.Canceled) || errors.Is(ctx.Err(), context.Canceled)
		return ShellResult{Err: err, ExitCode: -1, Cancelled: cancelled}
	}

	pgid := shellProcessGroupID(cmd.Process.Pid)
	up, _ := pty.(*xpty.UnixPty)

	// Read from PTY master in a goroutine.
	var (
		mu     sync.Mutex
		output strings.Builder
	)
	readDone := make(chan struct{})
	go func() {
		defer close(readDone)
		buf := make([]byte, 4096)
		for {
			n, readErr := pty.Read(buf)
			if n > 0 {
				chunk := string(buf[:n])
				mu.Lock()
				output.WriteString(chunk)
				mu.Unlock()
				if onChunk != nil {
					onChunk(chunk)
				}
			}
			if readErr != nil {
				return
			}
		}
	}()

	// Wait for process or cancellation.
	waitCh := make(chan error, 1)
	go func() {
		waitCh <- xpty.WaitProcess(ctx, cmd)
	}()

	var waitErr error
	select {
	case <-ctx.Done():
		// Kill the whole process group to unblock reads.
		if pgid > 0 {
			syscall.Kill(-pgid, syscall.SIGKILL) //nolint:errcheck
		} else if cmd.Process != nil {
			cmd.Process.Kill() //nolint:errcheck
		}
		_ = pty.Close()
		<-readDone
		<-waitCh
	case waitErr = <-waitCh:
		// Close slave so master sees EOF/EIO and read goroutine drains.
		if up != nil {
			_ = up.Slave().Close()
		}
		<-readDone
		_ = pty.Close()
	}

	cancelled := ctx.Err() != nil
	raw := output.String()
	// Normalize CRLF from PTY output, then strip trailing newlines and ANSI codes.
	clean := SanitizeStreamChunk(raw)
	clean = strings.TrimRight(clean, "\n")
	clean = ansi.Strip(clean)
	truncated := truncateShellOutput(clean)

	result := ShellResult{
		Output:    truncated,
		Cancelled: cancelled,
	}

	if waitErr != nil {
		if cancelled {
			return result
		}
		var exitErr *exec.ExitError
		if errors.As(waitErr, &exitErr) {
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
func FormatBashToolDetailBody(result toolresult.ToolResult, preferStream string) string {
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
	if gort.GOOS == "windows" {
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

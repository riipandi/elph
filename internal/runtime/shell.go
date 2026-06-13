package runtime

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"runtime"
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

	killOnCancel(ctx, cmd)

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

func killOnCancel(ctx context.Context, cmd *exec.Cmd) {
	go func() {
		<-ctx.Done()
		if cmd.Process == nil {
			return
		}
		if pgid, err := syscall.Getpgid(cmd.Process.Pid); err == nil {
			_ = syscall.Kill(-pgid, syscall.SIGKILL)
		}
		_ = cmd.Process.Kill()
	}()
}

// SanitizeStreamChunk normalizes streamed shell bytes for display.
func SanitizeStreamChunk(chunk string) string {
	return strings.NewReplacer("\r\n", "\n", "\r", "\n").Replace(chunk)
}

// TrimStreamOutput trims trailing whitespace from accumulated stream output.
func TrimStreamOutput(s string) string {
	return strings.TrimRight(s, "\n")
}

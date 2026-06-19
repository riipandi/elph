package git

import (
	"os/exec"
	"strconv"
	"strings"
)

// maxLineStatPaths caps how many changed paths get line stats computed.
// Beyond this, branch is still returned but line stats stay at zero.
const maxLineStatPaths = 32

// Status summarizes the current git branch and unstaged/staged line changes.
type Status struct {
	Branch  string
	Added   int
	Deleted int
	IsRepo  bool
}

// Read returns git metadata for workDir. Non-git directories use Branch "—".
func Read(workDir string) Status {
	if !isRepo(workDir) {
		return Status{Branch: "—"}
	}

	branch := getBranch(workDir)
	added, deleted := getLineStats(workDir)
	return Status{
		Branch:  branch,
		Added:   added,
		Deleted: deleted,
		IsRepo:  true,
	}
}

func isRepo(workDir string) bool {
	cmd := exec.Command("git", "rev-parse", "--is-inside-work-tree")
	cmd.Dir = workDir
	return cmd.Run() == nil
}

func getBranch(workDir string) string {
	cmd := exec.Command("git", "rev-parse", "--abbrev-ref", "HEAD")
	cmd.Dir = workDir
	out, err := cmd.Output()
	if err != nil {
		return "—"
	}
	branch := strings.TrimSpace(string(out))
	if branch == "" || branch == "HEAD" {
		return "detached"
	}
	return branch
}

func getLineStats(workDir string) (added, deleted int) {
	// Count changed files first to enforce the cap.
	cmd := exec.Command("git", "status", "--porcelain")
	cmd.Dir = workDir
	out, err := cmd.Output()
	if err != nil {
		return 0, 0
	}
	if countChangedFiles(string(out)) > maxLineStatPaths {
		return 0, 0
	}

	// Staged changes.
	a, d := numstat(workDir, "diff", "--cached", "--numstat")
	added += a
	deleted += d

	// Unstaged changes (tracked files only).
	a, d = numstat(workDir, "diff", "--numstat")
	added += a
	deleted += d

	return added, deleted
}

func countChangedFiles(porcelain string) int {
	n := 0
	for _, line := range strings.Split(strings.TrimSpace(porcelain), "\n") {
		if line == "" {
			continue
		}
		if len(line) < 2 {
			continue
		}
		// Skip untracked (??) and ignored (!!) entries.
		if line[0] == '?' && line[1] == '?' {
			continue
		}
		if line[0] == '!' && line[1] == '!' {
			continue
		}
		n++
	}
	return n
}

func numstat(workDir string, args ...string) (added, deleted int) {
	cmd := exec.Command("git", args...)
	cmd.Dir = workDir
	out, err := cmd.Output()
	if err != nil {
		return 0, 0
	}

	for _, line := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\t", 3)
		if len(parts) < 2 {
			continue
		}
		a, errA := strconv.Atoi(parts[0])
		d, errD := strconv.Atoi(parts[1])
		if errA != nil || errD != nil {
			continue
		}
		added += a
		deleted += d
	}
	return added, deleted
}

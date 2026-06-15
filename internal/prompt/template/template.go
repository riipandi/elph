package template

// Template is a markdown prompt snippet loaded from disk.
type Template struct {
	Name         string
	Description  string
	ArgumentHint string
	Content      string
	FilePath     string
	Scope        string // "global" or "project"
}

package websearch

import (
	"context"
	"io"
	"net/http"
)

// EngineDef describes a search backend for tests in other packages.
type EngineDef struct {
	ID          EngineID
	Name        string
	Rank        int
	RequiresKey bool
	KeyEnv      string
	Search      searchFunc
}

var defaultEngines []engine

func init() {
	defaultEngines = append([]engine(nil), engines...)
}

// SetEnginesForTest replaces the engine registry for the duration of a test.
func SetEnginesForTest(defs []EngineDef) {
	engines = make([]engine, len(defs))
	for i, d := range defs {
		engines[i] = engine{
			id:          d.ID,
			name:        d.Name,
			rank:        d.Rank,
			requiresKey: d.RequiresKey,
			keyEnv:      d.KeyEnv,
			search:      d.Search,
		}
	}
}

// ResetEnginesForTest restores the default engine registry.
func ResetEnginesForTest() {
	engines = append([]engine(nil), defaultEngines...)
}

// MockDuckDuckGoAt returns a search func that reads HTML results from a test server base URL.
func MockDuckDuckGoAt(baseURL string) searchFunc {
	return func(ctx context.Context, client *http.Client, query, _ string) ([]Result, error) {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseURL+"?q="+urlQueryEscape(query), nil)
		if err != nil {
			return nil, err
		}
		resp, err := client.Do(req)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()
		body, err := io.ReadAll(io.LimitReader(resp.Body, 2<<20))
		if err != nil {
			return nil, err
		}
		return parseDDGResults(string(body)), nil
	}
}

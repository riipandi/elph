package datastore

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/riipandi/elph/internal/settings"
	_ "turso.tech/database/tursogo"
)

var (
	globalDB   *sql.DB
	globalOnce sync.Once
	globalErr  error
)

// Open opens or creates the application database and runs pending migrations.
// It uses the configured database URL from settings, defaulting to ~/.elph/metadata.db.
// For Turso Cloud, provide database.url and database.token in settings.
func Open(ctx context.Context) (*sql.DB, error) {
	globalOnce.Do(func() {
		globalDB, globalErr = openDatabase(ctx)
	})
	return globalDB, globalErr
}

// MustOpen is like Open but panics on error.
func MustOpen(ctx context.Context) *sql.DB {
	db, err := Open(ctx)
	if err != nil {
		panic(fmt.Sprintf("datastore: failed to open database: %v", err))
	}
	return db
}

// DB returns the global database connection. Panics if Open has not been called.
func DB() *sql.DB {
	if globalDB == nil {
		panic("datastore: DB() called before Open()")
	}
	return globalDB
}

// Close closes the global database connection.
func Close() error {
	if globalDB != nil {
		return globalDB.Close()
	}
	return nil
}

func openDatabase(ctx context.Context) (*sql.DB, error) {
	prefs, err := settings.Load()
	if err != nil {
		return nil, fmt.Errorf("load settings: %w", err)
	}

	dbURL := prefs.DatabaseURL()
	dbToken := prefs.DatabaseToken()

	// Ensure parent directory exists for local databases
	if dirErr := ensureDBDir(dbURL); dirErr != nil {
		return nil, fmt.Errorf("ensure database directory: %w", dirErr)
	}

	// Build connection string based on configuration
	dsn := buildDSN(dbURL, dbToken)

	db, err := sql.Open("turso", dsn)
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}

	// Configure connection pool
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)

	// Verify connection
	if err := db.PingContext(ctx); err != nil {
		db.Close()
		return nil, fmt.Errorf("ping database: %w", err)
	}

	// Run migrations
	if err := RunMigrations(ctx, db); err != nil {
		db.Close()
		return nil, fmt.Errorf("run migrations: %w", err)
	}

	return db, nil
}

// buildDSN constructs the database connection string.
// For local files: file:path or just the path
// For Turso Cloud: libsql://url?authToken=token
func buildDSN(url, token string) string {
	// If it's a libsql URL, add the auth token
	if strings.HasPrefix(url, "libsql://") || strings.HasPrefix(url, "https://") {
		if token != "" {
			if strings.Contains(url, "?") {
				return url + "&authToken=" + token
			}
			return url + "?authToken=" + token
		}
		return url
	}

	// Local file - tursogo handles local paths directly
	return url
}

func ensureDBDir(dbURL string) error {
	// Skip for remote URLs
	if strings.HasPrefix(dbURL, "libsql://") || strings.HasPrefix(dbURL, "https://") {
		return nil
	}

	// Extract file path from file: scheme or use as-is
	path := strings.TrimPrefix(dbURL, "file:")

	// Create parent directory
	dir := filepath.Dir(path)
	if dir != "" && dir != "." {
		return os.MkdirAll(dir, 0o755)
	}
	return nil
}

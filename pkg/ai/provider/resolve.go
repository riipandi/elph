package provider

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// ResolveValueAllowMissingEnv resolves config values but treats unset environment
// variables as empty instead of returning an error.
func ResolveValueAllowMissingEnv(raw string) (string, error) {
	if raw == "" {
		return "", nil
	}
	if strings.HasPrefix(raw, "!") {
		return runConfigCommand(raw[1:])
	}
	return resolveInterpolatedAllowMissingEnv(raw)
}

// ResolveValue resolves Pi-style config values:
//   - !command — execute shell command, use stdout
//   - $ENV_VAR / ${ENV_VAR} / env.ENV_VAR — environment interpolation
//   - $$ — literal $
//   - $! — literal !
//   - otherwise — literal value
func ResolveValue(raw string) (string, error) {
	if raw == "" {
		return "", nil
	}
	if strings.HasPrefix(raw, "!") {
		return runConfigCommand(raw[1:])
	}
	return resolveInterpolated(raw)
}

// IsConfigured reports whether a config value is present without executing commands.
// Env references count as configured even when the variable is unset at check time.
func IsConfigured(raw string) bool {
	raw = strings.TrimSpace(raw)
	return raw != ""
}

func resolveInterpolated(raw string) (string, error) {
	return resolveInterpolatedWithLookup(raw, lookupEnv)
}

func resolveInterpolatedAllowMissingEnv(raw string) (string, error) {
	return resolveInterpolatedWithLookup(raw, lookupEnvOptional)
}

func resolveInterpolatedWithLookup(raw string, lookup func(string) (string, error)) (string, error) {
	var b strings.Builder
	for i := 0; i < len(raw); i++ {
		if raw[i] == '$' {
			consumed, value, err := resolveDollarRefWithLookup(raw, i, lookup)
			if err != nil {
				return "", err
			}
			if consumed == 0 {
				b.WriteByte('$')
				continue
			}
			b.WriteString(value)
			i += consumed - 1
			continue
		}
		if consumed, value, ok, err := tryResolveEnvDotRefWithLookup(raw, i, lookup); err != nil {
			return "", err
		} else if ok {
			b.WriteString(value)
			i += consumed - 1
			continue
		}
		b.WriteByte(raw[i])
	}
	return b.String(), nil
}

func resolveDollarRef(raw string, i int) (consumed int, value string, err error) {
	return resolveDollarRefWithLookup(raw, i, lookupEnv)
}

func resolveDollarRefWithLookup(raw string, i int, lookup func(string) (string, error)) (consumed int, value string, err error) {
	if i+1 >= len(raw) {
		return 0, "", nil
	}
	switch raw[i+1] {
	case '$':
		return 2, "$", nil
	case '!':
		return 2, "!", nil
	case '{':
		end := strings.IndexByte(raw[i+1:], '}')
		if end < 0 {
			return 0, "", fmt.Errorf("unterminated env reference in %q", raw)
		}
		name := raw[i+2 : i+1+end]
		resolved, err := lookup(name)
		if err != nil {
			return 0, "", err
		}
		return end + 2, resolved, nil
	default:
		j := i + 1
		for j < len(raw) {
			c := raw[j]
			if (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' {
				j++
				continue
			}
			break
		}
		if j == i+1 {
			return 0, "", nil
		}
		name := raw[i+1 : j]
		resolved, err := lookup(name)
		if err != nil {
			return 0, "", err
		}
		return j - i, resolved, nil
	}
}

func tryResolveEnvDotRef(raw string, i int) (consumed int, value string, ok bool, err error) {
	return tryResolveEnvDotRefWithLookup(raw, i, lookupEnv)
}

func tryResolveEnvDotRefWithLookup(raw string, i int, lookup func(string) (string, error)) (consumed int, value string, ok bool, err error) {
	if i+4 >= len(raw) || !strings.HasPrefix(raw[i:], "env.") {
		return 0, "", false, nil
	}
	j := i + 4
	for j < len(raw) {
		c := raw[j]
		if (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' {
			j++
			continue
		}
		break
	}
	if j == i+4 {
		return 0, "", false, nil
	}
	name := raw[i+4 : j]
	resolved, err := lookup(name)
	if err != nil {
		return 0, "", false, err
	}
	return j - i, resolved, true, nil
}

func lookupEnv(name string) (string, error) {
	value, ok := os.LookupEnv(name)
	if !ok {
		return "", fmt.Errorf("environment variable %q is not set", name)
	}
	return value, nil
}

func lookupEnvOptional(name string) (string, error) {
	value, ok := os.LookupEnv(name)
	if !ok {
		return "", nil
	}
	return value, nil
}

func runConfigCommand(command string) (string, error) {
	command = strings.TrimSpace(command)
	if command == "" {
		return "", fmt.Errorf("empty command")
	}
	cmd := exec.Command("sh", "-c", command)
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("command %q: %w", command, err)
	}
	return strings.TrimSpace(string(out)), nil
}

// ResolveHeaders resolves all header values, skipping entries that fail resolution.
func ResolveHeaders(headers map[string]string) (map[string]string, error) {
	return resolveHeadersWith(headers, ResolveValue)
}

func resolveHeadersAllowMissingEnv(headers map[string]string) (map[string]string, error) {
	return resolveHeadersWith(headers, ResolveValueAllowMissingEnv)
}

func resolveHeadersWith(headers map[string]string, resolve func(string) (string, error)) (map[string]string, error) {
	if len(headers) == 0 {
		return nil, nil
	}
	out := make(map[string]string, len(headers))
	for key, value := range headers {
		resolved, err := resolve(value)
		if err != nil {
			return nil, fmt.Errorf("header %q: %w", key, err)
		}
		if resolved != "" {
			out[key] = resolved
		}
	}
	return out, nil
}

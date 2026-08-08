# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `ui.narrowLogLines` setting (default `true`): collapsed tool-call items pack together in the
  transcript log (grouped/narrow log lines). Expanded (accessed) tool-call items, `Thinking`,
  and AI chat response/assistant items always keep line breaks above and below. Set to `false`
  for the classic blank-line spacing between every process-log row.

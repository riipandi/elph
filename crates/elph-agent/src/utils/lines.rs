//! Zero-copy newline splitting via `memchr`.

use memchr::memchr;

/// Count lines the same way `str::split('\n')` does.
pub fn count_lines(content: &str) -> usize {
    if content.is_empty() {
        1
    } else {
        memchr::memchr_iter(b'\n', content.as_bytes()).count() + 1
    }
}

/// Byte offsets where each line begins (same semantics as `str::split('\n')`).
pub fn line_starts(content: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(count_lines(content));
    starts.push(0);
    for pos in memchr::memchr_iter(b'\n', content.as_bytes()) {
        starts.push(pos + 1);
    }
    starts
}

/// Iterate over lines without allocating. Lines do not include trailing `\n`.
pub struct SplitLines<'a> {
    data: &'a str,
    start: usize,
    done: bool,
}

impl<'a> SplitLines<'a> {
    pub fn new(data: &'a str) -> Self {
        Self {
            data,
            start: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for SplitLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.start > self.data.len() {
            self.done = true;
            return None;
        }

        let remaining = &self.data[self.start..];
        match memchr(b'\n', remaining.as_bytes()) {
            Some(end) => {
                let line = &remaining[..end];
                self.start += end + 1;
                Some(line)
            }
            None => {
                self.done = true;
                Some(remaining)
            }
        }
    }
}

/// Collect line slices with a capacity hint derived from newline count.
pub fn lines_vec(content: &str) -> Vec<&str> {
    let mut lines = Vec::with_capacity(count_lines(content));
    lines.extend(SplitLines::new(content));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_matches_std_split() {
        for input in ["", "a", "a\n", "a\nb", "\n", "a\nb\n"] {
            let expected: Vec<&str> = input.split('\n').collect();
            let actual: Vec<&str> = SplitLines::new(input).collect();
            assert_eq!(expected, actual, "input={input:?}");
        }
    }

    #[test]
    fn line_starts_count_matches_split() {
        let input = "line 0\nline 1\nline 2";
        assert_eq!(line_starts(input).len(), SplitLines::new(input).count());
    }
}

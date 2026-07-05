//! Bracketed-paste aware stdin splitting (pi-tui `stdin-buffer.ts`).

const BRACKETED_PASTE_START: &str = "\x1b[200~";
const BRACKETED_PASTE_END: &str = "\x1b[201~";

/// Event emitted by [`StdinBuffer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(String),
    Paste(String),
}

/// Accumulates raw terminal input and splits bracketed paste regions.
#[derive(Default)]
pub struct StdinBuffer {
    pending: String,
    in_paste: bool,
}

impl StdinBuffer {
    pub fn push(&mut self, data: &str) -> Vec<InputEvent> {
        self.pending.push_str(data);
        let mut events = Vec::new();

        loop {
            if self.in_paste {
                if let Some(end) = self.pending.find(BRACKETED_PASTE_END) {
                    let paste = self.pending[..end].to_string();
                    self.pending.drain(..end + BRACKETED_PASTE_END.len());
                    self.in_paste = false;
                    events.push(InputEvent::Paste(paste));
                    continue;
                }
                break;
            }

            if let Some(start) = self.pending.find(BRACKETED_PASTE_START) {
                if start > 0 {
                    events.push(InputEvent::Key(self.pending[..start].to_string()));
                    self.pending.drain(..start);
                }
                self.pending.drain(..BRACKETED_PASTE_START.len());
                self.in_paste = true;
                continue;
            }

            if !self.pending.is_empty() {
                events.push(InputEvent::Key(std::mem::take(&mut self.pending)));
            }
            break;
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bracketed_paste() {
        let mut buf = StdinBuffer::default();
        let events = buf.push("hello\x1b[200~pasted\x1b[201~world");
        assert_eq!(
            events,
            vec![
                InputEvent::Key("hello".into()),
                InputEvent::Paste("pasted".into()),
                InputEvent::Key("world".into()),
            ]
        );
    }
}

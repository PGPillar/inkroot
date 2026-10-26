//! In-memory, flow-tagged log buffer that also forwards each line to the UI.

use crate::clock::Clock;
use crate::events::Emitter;
use crate::ipc::{LogLine, LogStream};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_LINES: usize = 20_000;

struct Inner {
    flow: Option<String>,
    lines: VecDeque<LogLine>,
}

pub struct LogStore {
    emitter: Arc<dyn Emitter>,
    clock: Arc<dyn Clock>,
    inner: Mutex<Inner>,
    /// Drops every line: neither stored nor forwarded (the idle device poller's adb chatter).
    discard: bool,
}

/// Forwards nothing.
struct NoEmit;

impl Emitter for NoEmit {
    fn device(&self, _: &crate::ipc::DeviceStatus) {}
    fn flow_state(&self, _: &crate::ipc::FlowState) {}
    fn progress(&self, _: &crate::ipc::Progress) {}
    fn log(&self, _: &LogLine) {}
}

fn stream_name(s: LogStream) -> &'static str {
    match s {
        LogStream::Stdout => "stdout",
        LogStream::Stderr => "stderr",
        LogStream::Info => "info",
    }
}

impl LogStore {
    pub fn new(emitter: Arc<dyn Emitter>, clock: Arc<dyn Clock>) -> Self {
        Self {
            emitter,
            clock,
            inner: Mutex::new(Inner {
                flow: None,
                lines: VecDeque::new(),
            }),
            discard: false,
        }
    }

    /// A store that drops every line, for tool output nobody should see (idle polling).
    pub fn discard() -> Self {
        Self {
            discard: true,
            ..Self::new(Arc::new(NoEmit), Arc::new(crate::clock::SystemClock::new()))
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_flow(&self, flow_id: Option<String>) {
        self.lock().flow = flow_id;
    }

    pub fn info(&self, line: &str) {
        self.line(LogStream::Info, line);
    }

    pub fn line(&self, stream: LogStream, line: &str) {
        if self.discard {
            return;
        }
        let ts = self
            .clock
            .wall()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
        let entry = {
            let mut g = self.lock();
            let entry = LogLine {
                flow_id: g.flow.clone(),
                ts,
                stream,
                line: line.to_string(),
            };
            g.lines.push_back(entry.clone());
            while g.lines.len() > MAX_LINES {
                g.lines.pop_front();
            }
            entry
        };
        // Emit outside the lock so a slow sink can't block other writers.
        self.emitter.log(&entry);
    }

    pub fn text(&self, flow_id: Option<&str>) -> String {
        let g = self.lock();
        let mut out = String::new();
        for l in g
            .lines
            .iter()
            .filter(|l| flow_id.is_none_or(|f| l.flow_id.as_deref() == Some(f)))
        {
            out.push_str(&format!(
                "{} [{}] {}\n",
                l.ts,
                stream_name(l.stream),
                l.line
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{DeviceStatus, FlowState, Progress};

    #[derive(Default)]
    struct Rec(Mutex<Vec<LogLine>>);
    impl Emitter for Rec {
        fn device(&self, _: &DeviceStatus) {}
        fn flow_state(&self, _: &FlowState) {}
        fn progress(&self, _: &Progress) {}
        fn log(&self, l: &LogLine) {
            self.0.lock().unwrap().push(l.clone());
        }
    }

    #[test]
    fn tags_and_filters() {
        let rec = Arc::new(Rec::default());
        let clock = Arc::new(crate::clock::FakeClock::new(chrono::Local::now()));
        let store = LogStore::new(rec.clone(), clock);
        store.set_flow(Some("f1".into()));
        store.info("a");
        store.set_flow(None);
        store.info("b");
        let t = store.text(Some("f1"));
        assert!(t.contains("a") && !t.contains("[info] b"));
        assert!(t.contains("[info] a\n"));
        let all = store.text(None);
        assert!(all.contains("[info] a\n") && all.contains("[info] b\n"));
        let got = rec.0.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].flow_id.as_deref(), Some("f1"));
        assert_eq!(got[1].flow_id, None);
    }

    #[test]
    fn discard_store_keeps_and_forwards_nothing() {
        let store = LogStore::discard();
        store.set_flow(Some("f1".into()));
        store.info("a");
        store.line(LogStream::Stdout, "b");
        assert_eq!(store.text(None), "");
    }

    #[test]
    fn buffer_is_capped() {
        let rec = Arc::new(Rec::default());
        let clock = Arc::new(crate::clock::FakeClock::new(chrono::Local::now()));
        let store = LogStore::new(rec, clock);
        for i in 0..20_005 {
            store.info(&format!("l{i}"));
        }
        let t = store.text(None);
        assert_eq!(t.lines().count(), 20_000);
        assert!(!t.contains("] l0\n") && t.contains("] l20004\n"));
    }
}

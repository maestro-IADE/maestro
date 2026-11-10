//! One event stream per worktree ("agent://event"), whatever runtime feeds it:
//! - `epoch`: bumped whenever the stream starts over (restart, another conversation, a truncated file; R4:
//!   chat ↔ CLI switch, fallback, replay). The UI drops what it holds and refetches.
//! - `seq`: the durable event's 1-based index in `runtime::history` of the current source.
//! - deltas are coalesced here (~50 ms) and flushed before any other event, so order is kept, and when the
//!   emitter is dropped, so a runtime's last deltas aren't lost. Only a delta starting where the pending text ends
//!   is joined to it (offsets in UTF-16 units), so folding stays idempotent.
//! - a record-keeping emitter (`with_record`, ACP in R4) keeps what it sent under the same lock (`history`).
//! A second counter, the PTY epoch, does the same for the terminal byte stream ("pty://data").

use crate::events::{Envelope, ResetReason, RuntimeEvent};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// How long text and thinking deltas are gathered before they go out together.
pub const DELTA_WINDOW: Duration = Duration::from_millis(50);
/// Pending delta text past this goes out at once.
const DELTA_MAX: usize = 64 * 1024;

pub struct Stream {
    pub epoch: u64,
    pub seq: u64,
    pub pty_epoch: u64,
    /// deltas not sent yet
    pending: Vec<RuntimeEvent>,
    pending_len: usize,
    first_delta: Option<Instant>,
    /// the emitter feeding it now (approvals push their events through it)
    emitter: Weak<Emitter>,
    /// durable events of this epoch sent by a record-keeping emitter (`Emitter::with_record`), and whose they are
    record: Vec<RuntimeEvent>,
    record_owner: u64,
}

static STREAMS: LazyLock<Mutex<HashMap<String, Arc<Mutex<Stream>>>>> = LazyLock::new(Mutex::default);

/// Worktree paths are compared case-insensitively, either slash.
fn key_of(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

fn stream(path: &str) -> Arc<Mutex<Stream>> {
    STREAMS
        .lock()
        .unwrap()
        .entry(key_of(path))
        .or_insert_with(|| Arc::new(Mutex::new(Stream { epoch: 1, seq: 0, pty_epoch: 1, pending: vec![], pending_len: 0, first_delta: None, emitter: Weak::new(), record: vec![], record_owner: 0 })))
        .clone()
}

/// The worktree's current chat epoch (1 until its first start this run).
pub fn epoch(path: &str) -> u64 {
    stream(path).lock().unwrap().epoch
}

/// The current PTY epoch (no terminal running: the last one's, or 1).
pub fn pty_epoch(path: &str) -> u64 {
    stream(path).lock().unwrap().pty_epoch
}

/// A new PTY under this key: its byte stream starts over.
pub fn bump_pty(path: &str) -> u64 {
    let s = stream(path);
    let mut s = s.lock().unwrap();
    s.pty_epoch += 1;
    s.pty_epoch
}

/// The emitter of the worktree's running agent, if any.
pub fn emitter(path: &str) -> Option<Arc<Emitter>> {
    stream(path).lock().unwrap().emitter.upgrade()
}

type Sink = Arc<dyn Fn(&Envelope) + Send + Sync>;

/// Where one runtime's events enter the stream: numbering, coalescing, and an observer that sees every durable
/// event in order (state dot, worker-done) before it goes out.
pub struct Emitter {
    /// tells emitters apart for the record (`Stream.record_owner`)
    id: u64,
    path: String,
    stream: Arc<Mutex<Stream>>,
    /// the epoch this emitter writes in; an emitter left over from a previous run writes nothing
    epoch: AtomicU64,
    sink: Sink,
    observer: Box<dyn Fn(&RuntimeEvent) + Send + Sync>,
    flusher: OnceLock<mpsc::Sender<()>>,
    /// keeps what it sends in `Stream.record` (`with_record`)
    record: bool,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// UTF-16 units, the unit a delta's offset counts in (the UI's string index).
fn utf16_len(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}

fn flush(s: &mut Stream, path: &str, sink: &Sink) {
    s.first_delta = None;
    s.pending_len = 0;
    for event in std::mem::take(&mut s.pending) {
        sink(&Envelope { worktree_path: path.into(), epoch: s.epoch, seq: None, event });
    }
}

impl Emitter {
    /// Becomes the worktree's emitter (`emitter()`). `sink` gets every envelope; `observer` every durable event.
    pub fn new(path: &str, sink: impl Fn(&Envelope) + Send + Sync + 'static, observer: impl Fn(&RuntimeEvent) + Send + Sync + 'static) -> Arc<Emitter> {
        Emitter::make(path, Arc::new(sink), Box::new(observer), false)
    }

    /// Like `new`, and keeps every durable event it sends, numbered as sent (`history`): for a runtime whose record
    /// can't be rebuilt from a transcript (ACP, R4). Kept in the stream, under the lock that assigns seq and epoch,
    /// so it can't drift from what went out. Don't `prime` it: the record numbers from its own first event.
    #[allow(dead_code)] // the ACP runtime (R4); tests
    pub fn with_record(path: &str, sink: impl Fn(&Envelope) + Send + Sync + 'static, observer: impl Fn(&RuntimeEvent) + Send + Sync + 'static) -> Arc<Emitter> {
        Emitter::make(path, Arc::new(sink), Box::new(observer), true)
    }

    fn make(path: &str, sink: Sink, observer: Box<dyn Fn(&RuntimeEvent) + Send + Sync>, record: bool) -> Arc<Emitter> {
        let stream = stream(path);
        let mut s = stream.lock().unwrap();
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let e = Arc::new(Emitter { id, path: path.into(), stream: stream.clone(), epoch: AtomicU64::new(s.epoch), sink, observer, flusher: OnceLock::new(), record });
        s.emitter = Arc::downgrade(&e);
        drop(s);
        e
    }

    /// The seq a durable event went out with; None for a live-only event, or when nothing went out (a stale
    /// emitter: a newer run took the stream).
    pub fn push(&self, ev: RuntimeEvent) -> Option<u64> {
        let mut s = self.stream.lock().unwrap();
        if s.epoch != self.epoch.load(Ordering::SeqCst) {
            return None;
        }
        match ev {
            RuntimeEvent::Delta { turn_id, item_id, channel, offset, text } => {
                let n = text.len();
                // joined only when it starts where the pending text ends: a repeated or overlapping piece (a retried
                // chunk, a reconnect) stays its own delta, so folding it at its offset stays idempotent
                match s.pending.last_mut() {
                    Some(RuntimeEvent::Delta { item_id: i, channel: c, offset: o, text: t, .. }) if *i == item_id && *c == channel && offset == *o + utf16_len(t) => t.push_str(&text),
                    _ => s.pending.push(RuntimeEvent::Delta { turn_id, item_id, channel, offset, text }),
                }
                s.pending_len += n;
                s.first_delta.get_or_insert_with(Instant::now);
                if s.pending_len > DELTA_MAX {
                    flush(&mut s, &self.path, &self.sink);
                } else {
                    self.arm();
                }
                None
            }
            ev if ev.durable() => {
                flush(&mut s, &self.path, &self.sink);
                s.seq += 1;
                let seq = s.seq;
                (self.observer)(&ev);
                if self.record {
                    // another emitter's record (same epoch, before our reset) isn't ours to extend
                    if s.record_owner != self.id {
                        s.record.clear();
                        s.record_owner = self.id;
                    }
                    s.record.push(ev.clone());
                }
                (self.sink)(&Envelope { worktree_path: self.path.clone(), epoch: s.epoch, seq: Some(seq), event: ev });
                Some(seq)
            }
            ev => {
                flush(&mut s, &self.path, &self.sink);
                (self.sink)(&Envelope { worktree_path: self.path.clone(), epoch: s.epoch, seq: None, event: ev });
                None
            }
        }
    }

    /// The epoch this emitter writes in (the stream's, unless a newer run took it).
    #[allow(dead_code)] // the ACP runtime (R4); tests
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// (epoch, durable events) of a record-keeping emitter (`with_record`), read under the lock that numbers them:
    /// element n-1 went out as seq n, in that epoch. None = no record, a newer run took the stream, or the record
    /// doesn't cover the epoch from its first seq (primed, or another emitter wrote in it first).
    #[allow(dead_code)] // the ACP runtime's `Runtime::history` (R4); tests
    pub fn history(&self) -> Option<(u64, Vec<RuntimeEvent>)> {
        let s = self.stream.lock().unwrap();
        let mine = self.record && s.epoch == self.epoch.load(Ordering::SeqCst) && s.record_owner == self.id && s.record.len() as u64 == s.seq;
        mine.then(|| (s.epoch, s.record.clone()))
    }

    /// Wakes the flusher, started on the first delta (a runtime that never sends deltas never starts one).
    fn arm(&self) {
        let tx = self.flusher.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<()>();
            let (stream, sink, path) = (Arc::downgrade(&self.stream), self.sink.clone(), self.path.clone());
            thread::spawn(move || {
                let mut deadline: Option<Instant> = None;
                loop {
                    let r = match deadline {
                        Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())),
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    };
                    let Some(stream) = stream.upgrade() else { break };
                    match r {
                        Ok(()) => {
                            if deadline.is_none() {
                                deadline = stream.lock().unwrap().first_delta.map(|t| t + DELTA_WINDOW);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            let mut s = stream.lock().unwrap();
                            // flushed meanwhile by a durable event, then new deltas came: wait for their own window
                            match s.first_delta {
                                Some(t) if t + DELTA_WINDOW > Instant::now() => deadline = Some(t + DELTA_WINDOW),
                                _ => {
                                    flush(&mut s, &path, &sink);
                                    deadline = None;
                                }
                            }
                        }
                        // the emitter is gone
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
            tx
        });
        let _ = tx.send(());
    }

    /// The stream starts over: pending deltas dropped, a new epoch, seq from 0, and a `Reset` saying why.
    /// Only the worktree's current emitter, in its own epoch, can do it: a previous run's last tick (its tail loop
    /// may run once more after the new run's `Emitter::new`) must not take the stream back.
    /// Returns the new epoch; None when refused.
    pub fn reset(&self, reason: ResetReason) -> Option<u64> {
        let mut s = self.stream.lock().unwrap();
        if !std::ptr::eq(s.emitter.as_ptr(), self) || s.epoch != self.epoch.load(Ordering::SeqCst) {
            return None;
        }
        s.pending.clear();
        s.pending_len = 0;
        s.first_delta = None;
        s.epoch += 1;
        s.seq = 0;
        s.record.clear();
        s.record_owner = if self.record { self.id } else { 0 };
        self.epoch.store(s.epoch, Ordering::SeqCst);
        (self.sink)(&Envelope { worktree_path: self.path.clone(), epoch: s.epoch, seq: None, event: RuntimeEvent::Reset { reason } });
        Some(s.epoch)
    }

    /// The first `n` durable events are already in the history the UI loads: numbering continues after them.
    /// (A record-keeping emitter's record then no longer covers the epoch: its `history` is None.)
    pub fn prime(&self, n: u64) {
        let mut s = self.stream.lock().unwrap();
        if s.epoch == self.epoch.load(Ordering::SeqCst) {
            s.seq = n;
        }
    }

    /// Sends pending deltas now.
    #[allow(dead_code)] // tests; the ACP runtime (R4) flushes at turn ends
    pub fn flush(&self) {
        let mut s = self.stream.lock().unwrap();
        flush(&mut s, &self.path, &self.sink);
    }
}

/// A runtime going away (shutdown, exit) sends what it still holds: the flusher stops with the emitter, so the
/// last window's deltas would otherwise stay pending until the next run's reset drops them. Never under the stream
/// lock: nothing holding it (sink, observer) keeps the last `Arc<Emitter>`.
impl Drop for Emitter {
    fn drop(&mut self) {
        if let Ok(mut s) = self.stream.lock() {
            if s.epoch == self.epoch.load(Ordering::SeqCst) && !s.pending.is_empty() {
                flush(&mut s, &self.path, &self.sink);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{DeltaChannel, NoticeKind};

    fn emitter(key: &str) -> (Arc<Mutex<Vec<Envelope>>>, Arc<Emitter>, Arc<Mutex<Vec<RuntimeEvent>>>) {
        let got = Arc::new(Mutex::new(vec![]));
        let seen = Arc::new(Mutex::new(vec![]));
        let (g, o) = (got.clone(), seen.clone());
        let e = Emitter::new(key, move |e: &Envelope| g.lock().unwrap().push(e.clone()), move |ev: &RuntimeEvent| o.lock().unwrap().push(ev.clone()));
        (got, e, seen)
    }

    fn delta(item: &str, channel: DeltaChannel, offset: u64, text: &str) -> RuntimeEvent {
        RuntimeEvent::Delta { turn_id: "t".into(), item_id: item.into(), channel, offset, text: text.into() }
    }

    fn note(t: &str) -> RuntimeEvent {
        RuntimeEvent::Notice { turn_id: "t".into(), item_id: t.into(), kind: NoticeKind::LocalCommand, text: t.into() }
    }

    #[test]
    fn adjacent_deltas_merge_and_keep_the_first_offset() {
        let (got, e, _) = emitter("test://merge");
        e.push(delta("a", DeltaChannel::Text, 0, "he"));
        e.push(delta("a", DeltaChannel::Text, 2, "llo"));
        e.push(delta("a", DeltaChannel::Thinking, 0, "hm"));
        e.push(delta("b", DeltaChannel::Text, 0, "x"));
        e.push(delta("a", DeltaChannel::Text, 5, "!"));
        assert!(got.lock().unwrap().is_empty());
        e.flush();
        let got = got.lock().unwrap();
        let ds: Vec<_> = got.iter().map(|e| &e.event).collect();
        assert_eq!(ds, [&delta("a", DeltaChannel::Text, 0, "hello"), &delta("a", DeltaChannel::Thinking, 0, "hm"), &delta("b", DeltaChannel::Text, 0, "x"), &delta("a", DeltaChannel::Text, 5, "!")]);
        assert!(got.iter().all(|e| e.seq.is_none()));
    }

    /// chatFold's way: each delta replaces the item's text from its offset (UTF-16 units)
    fn fold(deltas: &[RuntimeEvent]) -> String {
        let mut units: Vec<u16> = vec![];
        for d in deltas {
            if let RuntimeEvent::Delta { offset, text, .. } = d {
                units.truncate(*offset as usize);
                units.extend(text.encode_utf16());
            }
        }
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn only_contiguous_deltas_merge_so_folding_stays_idempotent() {
        let (got, e, _) = emitter("test://overlap");
        let sent = [
            delta("a", DeltaChannel::Text, 0, "he"),
            // resent as is (a retried chunk)
            delta("a", DeltaChannel::Text, 0, "he"),
            delta("a", DeltaChannel::Text, 2, "l가"),
            // overlaps what came before (a reconnect replaying from an earlier point)
            delta("a", DeltaChannel::Text, 3, "가lo"),
            // contiguous after a non-BMP char: offsets count UTF-16 units
            delta("a", DeltaChannel::Text, 6, "😀"),
            delta("a", DeltaChannel::Text, 8, "!"),
        ];
        for d in &sent {
            e.push(d.clone());
        }
        e.flush();
        let got = got.lock().unwrap();
        let out: Vec<RuntimeEvent> = got.iter().map(|g| g.event.clone()).collect();
        assert_eq!(fold(&out), fold(&sent));
        assert_eq!(fold(&out), "hel가lo😀!");
        assert_eq!(out, [delta("a", DeltaChannel::Text, 0, "he"), delta("a", DeltaChannel::Text, 0, "hel가"), delta("a", DeltaChannel::Text, 3, "가lo😀!")]);
    }

    #[test]
    fn a_record_keeping_emitter_matches_the_live_seq_across_a_reset() {
        let key = "test://record";
        let (got, s) = {
            let got = Arc::new(Mutex::new(vec![]));
            let g = got.clone();
            (got, move |e: &Envelope| g.lock().unwrap().push(e.clone()))
        };
        let e = Emitter::with_record(key, s, |_| {});
        assert_eq!(e.push(note("a")), Some(1));
        assert_eq!(e.push(delta("a", DeltaChannel::Text, 0, "x")), None);
        assert_eq!(e.push(note("b")), Some(2));
        assert_eq!(e.history(), Some((e.epoch(), vec![note("a"), note("b")])));
        let after = e.reset(ResetReason::Restart).unwrap();
        assert_eq!((after, e.epoch(), epoch(key)), (after, after, after));
        assert_eq!(e.history(), Some((after, vec![])));
        let last = e.push(note("c")).unwrap();
        let (h_epoch, h) = e.history().unwrap();
        // the record is exactly what went out live in this epoch, seq n = element n-1
        let live: Vec<_> = got.lock().unwrap().iter().filter(|g| g.epoch == after && g.seq.is_some()).map(|g| (g.seq.unwrap(), g.event.clone())).collect();
        assert_eq!((h_epoch, h.len() as u64), (after, last));
        assert_eq!(live, h.iter().cloned().enumerate().map(|(i, ev)| (i as u64 + 1, ev)).collect::<Vec<_>>());
        // a newer run took the stream: this emitter's record is no one's any more, and it writes nothing
        let (_, newer, _) = emitter(key);
        newer.reset(ResetReason::Restart);
        assert_eq!(e.push(note("late")), None);
        assert_eq!(e.history(), None);
        // an emitter without a record has no history
        assert_eq!(newer.history(), None);
    }

    #[test]
    fn durable_events_flush_deltas_first_and_take_a_seq() {
        let (got, e, seen) = emitter("test://order");
        e.push(delta("a", DeltaChannel::Text, 0, "he"));
        e.push(note("n1"));
        e.push(RuntimeEvent::Exited { turn_id: None });
        e.push(note("n2"));
        let got = got.lock().unwrap();
        assert!(matches!(got[0].event, RuntimeEvent::Delta { .. }));
        assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), [None, Some(1), None, Some(2)]);
        // the observer sees durable events only, before they go out
        assert_eq!(*seen.lock().unwrap(), [note("n1"), note("n2")]);
    }

    #[test]
    fn reset_drops_pending_and_starts_a_new_epoch() {
        let (got, e, _) = emitter("test://reset");
        let before = epoch("test://reset");
        e.prime(7);
        e.push(note("a"));
        e.push(delta("a", DeltaChannel::Text, 0, "lost"));
        e.reset(ResetReason::Restart);
        e.push(note("b"));
        e.flush();
        let got = got.lock().unwrap();
        assert_eq!(got[0].seq, Some(8));
        assert_eq!((got[1].epoch, got[1].seq, &got[1].event), (before + 1, None, &RuntimeEvent::Reset { reason: ResetReason::Restart }));
        assert_eq!((got[2].epoch, got[2].seq), (before + 1, Some(1)));
        assert_eq!(got.len(), 3);
        assert_eq!(epoch("TEST://Reset"), before + 1);
    }

    #[test]
    fn an_older_emitter_writes_nothing() {
        let (old_got, old, _) = emitter("test://stale");
        let (_, new, _) = emitter("test://stale");
        new.reset(ResetReason::Restart);
        old.push(note("late"));
        assert!(old_got.lock().unwrap().is_empty());
        assert!(Arc::ptr_eq(&super::emitter("test://stale").unwrap(), &new));
    }

    #[test]
    fn an_older_emitter_cannot_reset() {
        let (_, old, _) = emitter("test://stale-reset");
        let (new_got, new, _) = emitter("test://stale-reset");
        // before the new run's own reset (same epoch still) and after it
        old.reset(ResetReason::Truncated);
        new.reset(ResetReason::Restart);
        let e = epoch("test://stale-reset");
        old.reset(ResetReason::Source);
        assert_eq!(epoch("test://stale-reset"), e);
        new.push(note("live"));
        let got = new_got.lock().unwrap();
        assert_eq!(got.iter().filter(|g| matches!(g.event, RuntimeEvent::Reset { .. })).count(), 1);
        assert_eq!((got.last().unwrap().epoch, got.last().unwrap().seq), (e, Some(1)));
    }

    #[test]
    fn big_pending_text_goes_out_at_once() {
        let (got, e, _) = emitter("test://cap");
        e.push(delta("a", DeltaChannel::Text, 0, &"x".repeat(40 * 1024)));
        assert!(got.lock().unwrap().is_empty());
        e.push(delta("a", DeltaChannel::Text, 40 * 1024, &"y".repeat(30 * 1024)));
        assert_eq!(got.lock().unwrap().len(), 1);
    }

    #[test]
    fn deltas_go_out_after_the_window() {
        let (got, e, _) = emitter("test://window");
        e.push(delta("a", DeltaChannel::Text, 0, "a"));
        e.push(delta("a", DeltaChannel::Text, 1, "b"));
        assert!(got.lock().unwrap().is_empty());
        thread::sleep(DELTA_WINDOW + Duration::from_millis(80));
        let got = got.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].event, delta("a", DeltaChannel::Text, 0, "ab"));
    }

    #[test]
    fn a_dropped_emitter_sends_its_last_deltas() {
        let (got, e, _) = emitter("test://drop");
        e.push(delta("a", DeltaChannel::Text, 0, "tail"));
        assert!(got.lock().unwrap().is_empty());
        drop(e);
        let got = got.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].event, delta("a", DeltaChannel::Text, 0, "tail"));
        // nothing left for the next run to drop
        let (next_got, next, _) = emitter("test://drop");
        next.flush();
        assert!(next_got.lock().unwrap().is_empty());
    }

    #[test]
    fn pty_epochs_grow_per_start() {
        let a = bump_pty("test://pty");
        let b = bump_pty("test://PTY");
        assert!(a >= 2 && b == a + 1);
        assert_eq!(pty_epoch("test://pty"), b);
        // the chat epoch is its own counter
        assert_eq!(epoch("test://pty"), 1);
    }
}

//! looper.rs - record what you play, loop it, and stack more on top.
//!
//! Works like a guitarist's loop pedal:
//!
//!   record -> play a phrase -> close the loop -> it repeats -> overdub onto it
//!
//! WHAT IS RECORDED IS NOT AUDIO. Each event is "these pitches started this many
//! samples into the loop and were held this long", and playback performs them
//! again. That is a few bytes per chord rather than megabytes, it never degrades
//! however many times you overdub, and it means an arpeggiator switched on later
//! arpeggiates the recording too.
//!
//! The pitches are stored, not chord numbers. What you played was "chord 4 *of
//! C major*", and the key is half of that - so a loop stays put when you change
//! key to play over it, and a part overdubbed in A minor keeps its own key.
//!
//! ---------------------------------------------------------------------------
//! WHY THIS IS SHORTER THAN THE WEB VERSION
//! ---------------------------------------------------------------------------
//!
//! The browser looper needed a look-ahead scheduler: a JavaScript timer waking
//! every 25ms to queue notes 120ms into the future against the audio clock, plus
//! special handling so a throttled background tab didn't flush a burst of missed
//! notes on its return. None of that exists here. The audio thread calls `tick`
//! once per sample with the sample count, and an event fires on exactly the
//! sample it was recorded on. Time cannot drift from itself.
//!
//! This file knows nothing about sound - it is a state machine over sample
//! numbers, which is what makes it testable to the exact sample.

use crate::theory::Chord;

/// Most events one loop can hold. The storage is allocated once, up front, on
/// the thread that builds the engine - recording happens on the audio thread,
/// which must never allocate - and anything past this is quietly not recorded.
pub const MAX_EVENTS: usize = 2048;

/// How many chords can be held down at once while recording.
const MAX_OPEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LoopState {
    /// Nothing recorded.
    Idle = 0,
    /// Record pressed. Recording starts on the first chord, not on the button,
    /// so there is no dead air at the front of the loop while you get ready.
    Armed = 1,
    /// The first pass. The loop does not have a length yet.
    Recording = 2,
    Playing = 3,
    /// Playing, and recording on top.
    Overdub = 4,
    /// A loop exists but is not playing.
    Stopped = 5,
}

impl LoopState {
    /// For reading the state back out of the atomic the UI watches.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => LoopState::Armed,
            2 => LoopState::Recording,
            3 => LoopState::Playing,
            4 => LoopState::Overdub,
            5 => LoopState::Stopped,
            _ => LoopState::Idle,
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, LoopState::Playing | LoopState::Overdub)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopEvent {
    /// Samples from the start of the loop.
    pub t: u64,
    /// Samples it was held for.
    pub dur: u64,
    pub chord: Chord,
    /// Which pad it was, so that pad can light when the loop plays it.
    pub degree: u8,
}

/// A chord that has gone down while recording and not come back up yet.
#[derive(Clone, Copy)]
struct Open {
    id: u64,
    start: u64,
    chord: Chord,
    degree: u8,
}

pub struct Looper {
    pub state: LoopState,
    events: Vec<LoopEvent>,
    /// Recorded during an overdub pass, folded into `events` at the next wrap.
    /// The wrap is the one moment the playback cursor is about to reset to
    /// zero anyway, so re-sorting then can never skip or repeat an event.
    pending: Vec<LoopEvent>,
    open: [Option<Open>; MAX_OPEN],
    len: u64,
    rec_start: u64,
    play_start: u64,
    /// Index of the next event due in the current cycle.
    next: usize,
    /// Loop position on the previous tick; `None` means "a fresh cycle starts
    /// on the next tick".
    last_pos: Option<u64>,
    min_dur: u64,
}

impl Looper {
    /// `min_dur` is the shortest note a recording will keep, in samples, so a
    /// stab still gets its full attack when it is played back.
    pub fn new(min_dur: u64) -> Self {
        Looper {
            state: LoopState::Idle,
            events: Vec::with_capacity(MAX_EVENTS),
            pending: Vec::with_capacity(MAX_EVENTS),
            open: [None; MAX_OPEN],
            len: 0,
            rec_start: 0,
            play_start: 0,
            next: 0,
            last_pos: None,
            min_dur: min_dur.max(1),
        }
    }

    // Read by the tests only; the UI learns everything it needs from Status.
    #[cfg(test)]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[cfg(test)]
    pub fn event_count(&self) -> usize {
        self.events.len() + self.pending.len()
    }

    /// Samples into the loop at `now`. While recording the first pass there is
    /// no length to wrap at, so it simply counts up.
    fn pos(&self, now: u64) -> u64 {
        match self.state {
            LoopState::Recording => now - self.rec_start,
            _ if self.len > 0 => (now - self.play_start) % self.len,
            _ => 0,
        }
    }

    /// How far round the loop playback is, 0..1, for the progress bar.
    pub fn position(&self, now: u64) -> f32 {
        if self.state.is_running() && self.len > 0 {
            self.pos(now) as f32 / self.len as f32
        } else {
            0.0
        }
    }

    pub fn note_on(&mut self, now: u64, id: u64, chord: Chord, degree: u8) {
        if self.state == LoopState::Armed {
            self.state = LoopState::Recording;
            self.rec_start = now;
        }
        if !matches!(self.state, LoopState::Recording | LoopState::Overdub) {
            return;
        }
        let start = self.pos(now);
        if let Some(slot) = self.open.iter_mut().find(|s| s.is_none()) {
            *slot = Some(Open { id, start, chord, degree });
        }
    }

    pub fn note_off(&mut self, now: u64, id: u64) {
        let found = self
            .open
            .iter()
            .position(|s| matches!(s, Some(o) if o.id == id));
        if let Some(i) = found {
            let open = self.open[i].take().expect("slot was just matched");
            self.finish(now, open);
        }
    }

    fn finish(&mut self, now: u64, open: Open) {
        let end = self.pos(now);
        let dur = if end >= open.start {
            end - open.start
        } else {
            // Held across the end of the loop: the position wrapped, so end is
            // now smaller than start. Cut the note off at the boundary rather
            // than wrapping it - a note spilling into the next cycle would fight
            // with the copy of itself that starts there.
            // ponytail: truncate, don't wrap. If held-over notes ever matter
            // musically, split them into a second event at t=0.
            self.len.saturating_sub(open.start)
        };
        let event = LoopEvent {
            t: open.start,
            dur: dur.max(self.min_dur),
            chord: open.chord,
            degree: open.degree,
        };
        let target = if self.state == LoopState::Recording {
            &mut self.events
        } else {
            &mut self.pending
        };
        // Never grow past the preallocated capacity - growing would allocate.
        if target.len() < MAX_EVENTS {
            target.push(event);
        }
    }

    /// Every live input let go at once - a key or patch change, or the window
    /// losing focus. Chords still open in a recording end here rather than
    /// staying open until the loop closes and ringing to its end.
    pub fn release_all(&mut self, now: u64) {
        self.close_all(now);
    }

    fn close_all(&mut self, now: u64) {
        for i in 0..MAX_OPEN {
            if let Some(open) = self.open[i].take() {
                self.finish(now, open);
            }
        }
    }

    fn start_cycle(&mut self, now: u64, state: LoopState) {
        self.play_start = now;
        self.last_pos = None;
        self.next = 0;
        self.state = state;
    }

    /// The one button a loop pedal is built around.
    pub fn pedal(&mut self, now: u64) {
        match self.state {
            LoopState::Idle => self.state = LoopState::Armed,
            // Pressed again before playing anything - never mind.
            LoopState::Armed => self.state = LoopState::Idle,
            LoopState::Recording => {
                // Chords still held close at this instant, which is also the
                // instant the loop ends, so none can run past the end.
                self.close_all(now);
                let len = now - self.rec_start;
                if self.events.is_empty() || len < self.min_dur {
                    self.clear();
                    return;
                }
                self.len = len;
                self.events.sort_unstable_by_key(|e| e.t);
                self.start_cycle(now, LoopState::Playing);
            }
            LoopState::Playing => self.state = LoopState::Overdub,
            LoopState::Overdub => {
                self.close_all(now);
                self.state = LoopState::Playing;
            }
            // Record on a stopped loop: start it again, recording on top.
            LoopState::Stopped => self.start_cycle(now, LoopState::Overdub),
        }
    }

    pub fn play_stop(&mut self, now: u64) {
        match self.state {
            LoopState::Playing | LoopState::Overdub => {
                self.close_all(now);
                self.state = LoopState::Stopped;
            }
            LoopState::Stopped => self.start_cycle(now, LoopState::Playing),
            _ => {}
        }
    }

    pub fn clear(&mut self) {
        // `clear` keeps the capacity, so this does not free the storage the
        // audio thread relies on never having to allocate again.
        self.events.clear();
        self.pending.clear();
        self.open = [None; MAX_OPEN];
        self.len = 0;
        self.next = 0;
        self.last_pos = None;
        self.state = LoopState::Idle;
    }

    /// Call once per sample. `emit` receives every event that starts on this
    /// exact sample.
    pub fn tick(&mut self, now: u64, mut emit: impl FnMut(&LoopEvent)) {
        if !self.state.is_running() || self.len == 0 {
            return;
        }
        let pos = (now - self.play_start) % self.len;
        let wrapped = match self.last_pos {
            None => true,
            Some(prev) => pos < prev,
        };
        if wrapped {
            if !self.pending.is_empty() {
                let room = MAX_EVENTS - self.events.len();
                self.pending.truncate(room);
                self.events.extend_from_slice(&self.pending);
                self.pending.clear();
                // sort_unstable sorts in place; the stable sort allocates a
                // scratch buffer, which this thread is not allowed to do.
                self.events.sort_unstable_by_key(|e| e.t);
            }
            self.next = 0;
        }
        self.last_pos = Some(pos);

        while self.next < self.events.len() && self.events[self.next].t <= pos {
            emit(&self.events[self.next]);
            self.next += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 10;

    fn c(root: i32) -> Chord {
        Chord::new(&[root, root + 4, root + 7])
    }

    /// Run the looper from `from` to `to` (exclusive) and collect (sample, degree)
    /// for everything it emits.
    fn run(l: &mut Looper, from: u64, to: u64) -> Vec<(u64, u8)> {
        let mut out = vec![];
        for now in from..to {
            l.tick(now, |e| out.push((now, e.degree)));
        }
        out
    }

    /// Record chord 0 at 100..300 and chord 4 at 400..600, closing at 1000.
    fn two_chord_loop() -> Looper {
        let mut l = Looper::new(MIN);
        l.pedal(0);
        l.note_on(100, 1, c(60), 0);
        l.note_off(300, 1);
        l.note_on(400, 2, c(67), 4);
        l.note_off(600, 2);
        l.pedal(1000);
        l
    }

    #[test]
    fn nothing_is_recorded_until_record_is_pressed() {
        let mut l = Looper::new(MIN);
        l.note_on(10, 1, c(60), 0);
        l.note_off(50, 1);
        assert_eq!(l.state, LoopState::Idle);
        assert_eq!(l.event_count(), 0);
    }

    #[test]
    fn the_loop_starts_on_the_first_chord_not_the_button() {
        let mut l = Looper::new(MIN);
        l.pedal(0);
        assert_eq!(l.state, LoopState::Armed);
        l.note_on(5000, 1, c(60), 0);
        assert_eq!(l.state, LoopState::Recording);
        l.note_off(5200, 1);
        l.pedal(6000);
        // Length runs from the first chord, not from the press at 0.
        assert_eq!(l.len(), 1000);
    }

    #[test]
    fn pressing_record_twice_without_playing_cancels() {
        let mut l = Looper::new(MIN);
        l.pedal(0);
        l.pedal(10);
        assert_eq!(l.state, LoopState::Idle);
    }

    #[test]
    fn closing_the_loop_records_offsets_and_durations() {
        let l = two_chord_loop();
        assert_eq!(l.state, LoopState::Playing);
        assert_eq!(l.len(), 900);
        assert_eq!(l.events[0].t, 0);
        assert_eq!(l.events[0].dur, 200);
        assert_eq!(l.events[1].t, 300);
        assert_eq!(l.events[1].dur, 200);
    }

    #[test]
    fn events_store_the_pitches_they_were_played_with() {
        // Pitches, not chord numbers - what keeps a loop in its own key.
        let l = two_chord_loop();
        assert_eq!(l.events[0].chord, c(60));
        assert_eq!(l.events[1].chord, c(67));
    }

    #[test]
    fn playback_lands_on_the_exact_recorded_sample_every_cycle() {
        let mut l = two_chord_loop(); // playing from sample 1000, length 900
        let fired = run(&mut l, 1000, 1000 + 900 * 3);
        assert_eq!(
            fired,
            vec![(1000, 0), (1300, 4), (1900, 0), (2200, 4), (2800, 0), (3100, 4)]
        );
    }

    #[test]
    fn a_chord_held_while_closing_the_loop_ends_with_it() {
        let mut l = Looper::new(MIN);
        l.pedal(0);
        l.note_on(0, 1, c(60), 0);
        l.pedal(500); // still held
        assert_eq!(l.events[0].dur, 500);
    }

    #[test]
    fn overdub_waits_for_the_wrap_then_plays_in_time() {
        let mut l = two_chord_loop();
        run(&mut l, 1000, 1500);
        l.pedal(1500); // -> overdub, at loop position 500
        assert_eq!(l.state, LoopState::Overdub);
        l.note_on(1600, 9, c(65), 3); // position 600
        l.note_off(1700, 9);

        // Not heard in the cycle it was played in...
        let rest = run(&mut l, 1500, 1900);
        assert!(rest.iter().all(|&(_, d)| d != 3), "overdub played early: {rest:?}");

        // ...then joins the loop, in order, at its recorded position.
        let next = run(&mut l, 1900, 2800);
        assert_eq!(next, vec![(1900, 0), (2200, 4), (2500, 3)]);
    }

    #[test]
    fn a_note_held_over_the_loop_end_is_cut_at_the_boundary() {
        let mut l = two_chord_loop(); // length 900
        run(&mut l, 1000, 1800);
        l.pedal(1800); // overdub at position 800
        l.note_on(1800, 9, c(65), 3);
        run(&mut l, 1800, 2000); // wraps at 1900
        l.note_off(2000, 9); // position 100, i.e. after the wrap
        l.pedal(2000);
        run(&mut l, 2000, 2801); // through the wrap at 2800, which folds the overdub in
        let ev = l.events.iter().find(|e| e.degree == 3).expect("overdub kept");
        assert_eq!(ev.t, 800);
        assert_eq!(ev.dur, 100, "should stop at the loop end, 900 - 800");
    }

    #[test]
    fn stop_silences_and_play_restarts_from_the_top() {
        let mut l = two_chord_loop();
        run(&mut l, 1000, 1400);
        l.play_stop(1400);
        assert_eq!(l.state, LoopState::Stopped);
        assert!(run(&mut l, 1400, 5000).is_empty(), "a stopped loop played");

        l.play_stop(5000);
        assert_eq!(run(&mut l, 5000, 5400), vec![(5000, 0), (5300, 4)]);
    }

    #[test]
    fn clear_forgets_everything() {
        let mut l = two_chord_loop();
        l.clear();
        assert_eq!(l.state, LoopState::Idle);
        assert_eq!(l.event_count(), 0);
        assert!(run(&mut l, 0, 5000).is_empty());
    }

    #[test]
    fn closing_an_empty_recording_goes_back_to_idle() {
        let mut l = Looper::new(MIN);
        l.pedal(0);
        l.note_on(100, 1, c(60), 0);
        // Closing with a chord still open records it, so drop the note first
        // by clearing, then try the genuinely empty case: arm and close.
        l.clear();
        l.pedal(0);
        l.pedal(10);
        assert_eq!(l.state, LoopState::Idle);
    }

    #[test]
    fn position_runs_zero_to_one_round_the_loop() {
        let l = two_chord_loop();
        assert_eq!(l.position(1000), 0.0);
        assert!((l.position(1450) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn recording_never_grows_past_the_preallocated_storage() {
        // Growing the Vec would allocate on the audio thread. Hammer it with
        // far more chords than fit and check the allocation never moved.
        let mut l = Looper::new(MIN);
        let cap = l.events.capacity();
        l.pedal(0);
        for i in 0..(MAX_EVENTS as u64 * 2) {
            l.note_on(i * 20, i, c(60), 0);
            l.note_off(i * 20 + 15, i);
        }
        assert_eq!(l.events.len(), MAX_EVENTS);
        assert_eq!(l.events.capacity(), cap, "events reallocated");
    }
}

//! looper.rs - record what you play, loop it, and stack layers on top.
//!
//! Built for someone who has never used a loop pedal:
//!
//!   record -> play -> it stops by itself after N bars and starts looping
//!          -> record again -> one more pass, on top, as a new LAYER
//!
//! Three things make that easy rather than fiddly:
//!
//! 1. THE LOOP IS A WHOLE NUMBER OF BARS. A pedal whose loop is "however long
//!    you held it" needs you to hit the button on the exact beat, and a loop
//!    closed a little late hiccups on every repeat - under every layer you
//!    ever add. Here the length is chosen up front (1, 2, 4 or 8 bars at the
//!    tempo) and recording ends itself. Pressing record early finishes at the
//!    end of the current bar, never mid-bar.
//!
//! 2. NOTES SNAP TO THE BEAT. Starts and ends are rounded to the nearest eighth
//!    note, so a chord played a touch early or late loops in time anyway.
//!
//! 3. EVERY PASS IS ITS OWN LAYER, one loop long, and it keeps the sound and
//!    arpeggiator setting it was recorded with - so a pad layer, then a pluck
//!    arpeggio on top, then a bass line, each stays what it was. A bad layer is
//!    undone on its own; a layer can be muted to hear the others without it.
//!
//! WHAT IS RECORDED IS NOT AUDIO. Each event is "these pitches started this many
//! samples into the loop and were held this long", and playback performs them
//! again. That is a few bytes per chord rather than megabytes, it never degrades
//! however many layers you stack, and deleting a layer is deleting its events.
//!
//! The pitches are stored, not chord numbers. What you played was "chord 4 *of
//! C major*", and the key is half of that - so a loop stays put when you change
//! key to play over it, and a layer played in A minor keeps its own key.
//!
//! This file knows nothing about sound - it is a state machine over sample
//! numbers, which is what makes it testable to the exact sample. The audio
//! thread calls `tick` once per sample, and an event fires on exactly the
//! sample it belongs to.

use crate::theory::Chord;

/// Most events one loop can hold. The storage is allocated once, up front, on
/// the thread that builds the engine - recording happens on the audio thread,
/// which must never allocate - and anything past this is quietly not recorded.
pub const MAX_EVENTS: usize = 2048;

/// Layers one loop can stack. Eight is more than a beginner arrangement needs
/// and still fits one row of buttons.
pub const MAX_LAYERS: usize = 8;

/// Loop lengths on offer, in bars.
pub const BAR_CHOICES: [u8; 4] = [1, 2, 4, 8];

/// The grid notes snap to is eighth notes - the arpeggiator's step, so an
/// arpeggiated layer and a block-chord layer land on the same grid.
pub const STEPS_PER_BEAT: u64 = 2;
/// Four beats in a bar: 4/4, which nearly everything you will play along to is.
pub const STEPS_PER_BAR: u64 = 4 * STEPS_PER_BEAT;

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
    /// The first layer. Ends by itself once the chosen number of bars is up.
    Recording = 2,
    Playing = 3,
    /// Playing, and recording one more layer on top - for one pass.
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

    /// Whether a loop exists at all - and so its tempo is locked in.
    pub fn has_loop(self) -> bool {
        !matches!(self, LoopState::Idle | LoopState::Armed)
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
    /// The sound it was played with.
    pub patch: u8,
    /// Whether it was arpeggiated when it was played.
    pub arp: bool,
    pub layer: u8,
}

/// What a layer was recorded with, for its button in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Layer {
    pub patch: u8,
    pub arp: bool,
    pub muted: bool,
}

/// A chord that has gone down while recording and not come back up yet.
#[derive(Clone, Copy)]
struct Open {
    id: u64,
    start: u64,
    chord: Chord,
    degree: u8,
    patch: u8,
    arp: bool,
}

pub struct Looper {
    pub state: LoopState,
    /// The length the next recording will be. Changing it never touches a loop
    /// that already exists.
    pub bars: u8,
    events: Vec<LoopEvent>,
    /// Recorded during an overdub pass, folded into `events` at the next wrap.
    /// The wrap is the one moment the playback cursor is about to reset to
    /// zero anyway, so re-sorting then can never skip or repeat an event.
    pending: Vec<LoopEvent>,
    open: [Option<Open>; MAX_OPEN],
    layers: [Layer; MAX_LAYERS],
    /// Finished layers. The one being recorded, if any, is `layers[layer_count]`.
    layer_count: usize,
    /// Chords played into the layer being recorded. A pass with none in it
    /// leaves no layer behind.
    rec_notes: usize,
    /// One eighth note, in samples.
    step: u64,
    len: u64,
    rec_start: u64,
    /// Where the first recording will end.
    rec_len: u64,
    play_start: u64,
    /// When the current overdub pass began; it ends one loop later.
    pass_start: u64,
    /// Index of the next event due in the current cycle.
    next: usize,
    /// Loop position on the previous tick; `None` means "a fresh cycle starts
    /// on the next tick".
    last_pos: Option<u64>,
    min_dur: u64,
}

impl Looper {
    /// `min_dur` is the shortest note a recording will keep, in samples, so a
    /// stab still gets its full attack when it is played back. `step` is one
    /// eighth note in samples.
    pub fn new(min_dur: u64, step: u64) -> Self {
        Looper {
            state: LoopState::Idle,
            bars: 4,
            events: Vec::with_capacity(MAX_EVENTS),
            pending: Vec::with_capacity(MAX_EVENTS),
            open: [None; MAX_OPEN],
            layers: [Layer::default(); MAX_LAYERS],
            layer_count: 0,
            rec_notes: 0,
            step: step.max(1),
            len: 0,
            rec_start: 0,
            rec_len: 0,
            play_start: 0,
            pass_start: 0,
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

    /// A new tempo. Ignored once a loop exists: its notes sit on the old grid,
    /// and stretching them would be a different instrument.
    pub fn set_step(&mut self, step: u64) {
        if !self.state.has_loop() {
            self.step = step.max(1);
        }
    }

    fn bar_len(&self) -> u64 {
        self.step * STEPS_PER_BAR
    }

    /// Length of the loop in bars - or, while the first layer records, the
    /// length it is going to be.
    pub fn loop_bars(&self) -> u8 {
        let len = if self.state == LoopState::Recording { self.rec_len } else { self.len };
        (len / self.bar_len()) as u8
    }

    /// Layer `k` and whether it is the one being recorded right now.
    pub fn layer(&self, k: usize) -> Option<(Layer, bool)> {
        let recording = matches!(self.state, LoopState::Recording | LoopState::Overdub);
        if k < self.layer_count {
            Some((self.layers[k], false))
        } else if k == self.layer_count && recording && self.rec_notes > 0 {
            Some((self.layers[k], true))
        } else {
            None
        }
    }

    /// Samples into the loop at `now`. While recording the first layer there
    /// is no loop to wrap round yet, so it simply counts up.
    fn pos(&self, now: u64) -> u64 {
        match self.state {
            LoopState::Recording => now - self.rec_start,
            _ if self.len > 0 => (now - self.play_start) % self.len,
            _ => 0,
        }
    }

    /// How far round the loop playback is, 0..1, for the progress bar. While
    /// the first layer records, how much of it is done.
    pub fn position(&self, now: u64) -> f32 {
        match self.state {
            LoopState::Recording if self.rec_len > 0 => {
                (now - self.rec_start) as f32 / self.rec_len as f32
            }
            s if s.is_running() && self.len > 0 => self.pos(now) as f32 / self.len as f32,
            _ => 0.0,
        }
    }

    /// While recording, `Some(true)` on the first beat of a bar, `Some(false)`
    /// on the other beats - the metronome's cue. `None` otherwise.
    pub fn beat(&self, now: u64) -> Option<bool> {
        if !matches!(self.state, LoopState::Recording | LoopState::Overdub) {
            return None;
        }
        let pos = self.pos(now);
        (pos % (self.step * STEPS_PER_BEAT) == 0).then(|| pos % self.bar_len() == 0)
    }

    pub fn note_on(&mut self, now: u64, id: u64, chord: Chord, degree: u8, patch: u8, arp: bool) {
        if self.state == LoopState::Armed {
            self.state = LoopState::Recording;
            self.rec_start = now;
            self.rec_len = self.bars.max(1) as u64 * self.bar_len();
        }
        if !matches!(self.state, LoopState::Recording | LoopState::Overdub) {
            return;
        }
        if self.rec_notes == 0 {
            // The layer takes the sound of its first chord for its label.
            self.layers[self.layer_count] = Layer { patch, arp, muted: false };
        }
        self.rec_notes += 1;
        let start = self.pos(now);
        if let Some(slot) = self.open.iter_mut().find(|s| s.is_none()) {
            *slot = Some(Open { id, start, chord, degree, patch, arp });
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
        let cycle = if self.state == LoopState::Recording { self.rec_len } else { self.len };
        let pos = self.pos(now);
        // Held across the end of the loop: the position wrapped, so it is now
        // behind where the note started. Cut the note off at the boundary
        // rather than wrapping it - a note spilling into the next cycle would
        // fight with the copy of itself that starts there.
        // ponytail: truncate, don't wrap. If held-over notes ever matter
        // musically, split them into a second event at t=0.
        let end = if pos >= open.start { pos } else { cycle };

        // Snap both ends to the nearest eighth note. Snapping the end as well
        // as the start is what keeps a chord from overlapping the next one by
        // the half-step its start was nudged.
        let step = self.step;
        let snap = |x: u64| (x + step / 2) / step * step;
        let (start, snapped_end) = (snap(open.start), snap(end));
        let dur = if snapped_end > start { snapped_end - start } else { end - open.start };
        // Snapped up to the very end of the loop is the start of the next one.
        let t = if cycle > 0 && start >= cycle { start - cycle } else { start };

        let event = LoopEvent {
            t,
            dur: dur.max(self.min_dur),
            chord: open.chord,
            degree: open.degree,
            patch: open.patch,
            arp: open.arp,
            layer: self.layer_count as u8,
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
    /// staying open until the pass ends and ringing to its end.
    pub fn release_all(&mut self, now: u64) {
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

    fn begin_pass(&mut self, now: u64) {
        self.state = LoopState::Overdub;
        self.pass_start = now;
        self.rec_notes = 0;
        self.layers[self.layer_count] = Layer::default();
    }

    /// The first layer is finished: it becomes the loop.
    fn close_first_layer(&mut self, now: u64) {
        self.release_all(now);
        let len = self.rec_len;
        for e in self.events.iter_mut() {
            // An early press of record can shorten the loop after a note near
            // the old end had already snapped onto it.
            e.t %= len;
            e.dur = e.dur.min(len - e.t).max(self.min_dur);
        }
        self.events.sort_unstable_by_key(|e| e.t);
        self.len = len;
        self.layer_count = 1;
        self.rec_notes = 0;
        self.start_cycle(now, LoopState::Playing);
    }

    fn end_pass(&mut self, now: u64) {
        self.release_all(now);
        if self.rec_notes > 0 {
            self.layer_count += 1;
        }
        self.rec_notes = 0;
        self.state = LoopState::Playing;
    }

    /// The record button.
    pub fn pedal(&mut self, now: u64) {
        let room = self.layer_count < MAX_LAYERS;
        match self.state {
            LoopState::Idle => self.state = LoopState::Armed,
            // Pressed again before playing anything - never mind.
            LoopState::Armed => self.state = LoopState::Idle,
            LoopState::Recording => {
                // Finish at the end of the bar being played, so the loop is
                // still a whole number of bars.
                let bar = self.bar_len();
                let bars = (now - self.rec_start).div_ceil(bar).max(1);
                self.rec_len = self.rec_len.min(bars * bar);
            }
            LoopState::Playing if room => self.begin_pass(now),
            LoopState::Overdub => self.end_pass(now),
            // Record on a stopped loop: start it again, recording on top.
            LoopState::Stopped if room => {
                self.start_cycle(now, LoopState::Playing);
                self.begin_pass(now);
            }
            LoopState::Playing | LoopState::Stopped => {}
        }
    }

    pub fn play_stop(&mut self, now: u64) {
        match self.state {
            LoopState::Armed => self.state = LoopState::Idle,
            LoopState::Overdub => {
                self.end_pass(now);
                self.state = LoopState::Stopped;
            }
            LoopState::Playing => self.state = LoopState::Stopped,
            LoopState::Stopped => self.start_cycle(now, LoopState::Playing),
            LoopState::Idle | LoopState::Recording => {}
        }
    }

    /// Take back the newest layer - the one being recorded, if there is one.
    pub fn undo(&mut self) {
        match self.state {
            LoopState::Armed | LoopState::Recording => self.clear(),
            LoopState::Overdub if self.rec_notes > 0 => self.remove_layer(self.layer_count),
            _ => {
                // An overdub pass with nothing played yet ends with the undo.
                if self.state == LoopState::Overdub {
                    self.state = LoopState::Playing;
                }
                if self.layer_count > 0 {
                    self.remove_layer(self.layer_count - 1);
                }
            }
        }
    }

    /// Delete one layer, leaving the others exactly where they were.
    pub fn remove_layer(&mut self, k: usize) {
        if matches!(self.state, LoopState::Armed | LoopState::Recording) {
            self.clear();
            return;
        }
        let recording = self.state == LoopState::Overdub;
        if k > self.layer_count || (k == self.layer_count && !recording) {
            return;
        }

        // `retain` works in place, so this allocates nothing.
        self.events.retain(|e| e.layer as usize != k);
        self.pending.retain(|e| e.layer as usize != k);
        for e in self.events.iter_mut().chain(self.pending.iter_mut()) {
            if e.layer as usize > k {
                e.layer -= 1;
            }
        }
        self.layers.copy_within(k + 1.., k);
        self.layers[MAX_LAYERS - 1] = Layer::default();

        if k == self.layer_count {
            // The layer being recorded: throw it away and stop recording.
            self.open = [None; MAX_OPEN];
            self.rec_notes = 0;
            self.state = LoopState::Playing;
        } else {
            self.layer_count -= 1;
        }

        let still_recording = self.state == LoopState::Overdub && self.rec_notes > 0;
        if self.layer_count == 0 && !still_recording {
            self.clear();
            return;
        }
        // Events shifted down the list; find this cycle's place in it again.
        self.next = match self.last_pos {
            Some(p) => self.events.partition_point(|e| e.t <= p),
            None => 0,
        };
    }

    pub fn toggle_mute(&mut self, k: usize) {
        if k < MAX_LAYERS {
            self.layers[k].muted = !self.layers[k].muted;
        }
    }

    pub fn clear(&mut self) {
        // `clear` keeps the capacity, so this does not free the storage the
        // audio thread relies on never having to allocate again.
        self.events.clear();
        self.pending.clear();
        self.open = [None; MAX_OPEN];
        self.layers = [Layer::default(); MAX_LAYERS];
        self.layer_count = 0;
        self.rec_notes = 0;
        self.len = 0;
        self.next = 0;
        self.last_pos = None;
        self.state = LoopState::Idle;
    }

    /// Call once per sample. `emit` receives every event that starts on this
    /// exact sample.
    pub fn tick(&mut self, now: u64, mut emit: impl FnMut(&LoopEvent)) {
        match self.state {
            LoopState::Recording if now - self.rec_start >= self.rec_len => {
                self.close_first_layer(now)
            }
            LoopState::Overdub if now - self.pass_start >= self.len => self.end_pass(now),
            _ => {}
        }
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
            let e = &self.events[self.next];
            if !self.layers[e.layer as usize].muted {
                emit(e);
            }
            self.next += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 10;
    /// An eighth note of 100 samples: a beat is 200, a bar 800.
    const STEP: u64 = 100;
    const BAR: u64 = 800;

    fn c(root: i32) -> Chord {
        Chord::new(&[root, root + 4, root + 7])
    }

    fn looper(bars: u8) -> Looper {
        let mut l = Looper::new(MIN, STEP);
        l.bars = bars;
        l
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

    fn play(l: &mut Looper, on: u64, off: u64, id: u64, degree: u8) {
        l.note_on(on, id, c(60 + degree as i32), degree, 0, false);
        l.note_off(off, id);
    }

    /// One bar: chord 0 at 0..200, chord 4 at 400..600. Loops from sample 800.
    fn one_bar_loop() -> Looper {
        let mut l = looper(1);
        l.pedal(0);
        play(&mut l, 0, 200, 1, 0);
        play(&mut l, 400, 600, 2, 4);
        run(&mut l, 0, 801);
        l
    }

    #[test]
    fn nothing_is_recorded_until_record_is_pressed() {
        let mut l = looper(1);
        play(&mut l, 10, 50, 1, 0);
        assert_eq!(l.state, LoopState::Idle);
        assert_eq!(l.event_count(), 0);
    }

    #[test]
    fn recording_starts_on_the_first_chord_and_stops_itself_after_the_bars() {
        let mut l = looper(2);
        l.pedal(0);
        assert_eq!(l.state, LoopState::Armed);
        run(&mut l, 0, 5000);
        l.note_on(5000, 1, c(60), 0, 0, false);
        assert_eq!(l.state, LoopState::Recording);
        l.note_off(5200, 1);
        run(&mut l, 5000, 5000 + 2 * BAR);
        assert_eq!(l.state, LoopState::Recording, "closed a sample early");
        run(&mut l, 5000 + 2 * BAR, 5000 + 2 * BAR + 1);
        assert_eq!(l.state, LoopState::Playing);
        assert_eq!(l.len(), 2 * BAR);
    }

    #[test]
    fn pressing_record_twice_without_playing_cancels() {
        let mut l = looper(1);
        l.pedal(0);
        l.pedal(10);
        assert_eq!(l.state, LoopState::Idle);
    }

    #[test]
    fn pressing_record_early_finishes_at_the_end_of_that_bar() {
        let mut l = looper(8);
        l.pedal(0);
        play(&mut l, 0, 100, 1, 0);
        l.pedal(BAR + 250); // a quarter of the way into bar two
        run(&mut l, 0, 2 * BAR);
        assert_eq!(l.state, LoopState::Recording);
        run(&mut l, 2 * BAR, 2 * BAR + 1);
        assert_eq!(l.len(), 2 * BAR);
    }

    #[test]
    fn notes_snap_to_the_nearest_eighth_note() {
        let mut l = looper(1);
        l.pedal(0);
        play(&mut l, 0, 190, 1, 0); // ends 10 early
        play(&mut l, 240, 395, 2, 4); // starts 40 late, ends 5 early
        run(&mut l, 0, 801);
        assert_eq!((l.events[0].t, l.events[0].dur), (0, 200));
        assert_eq!((l.events[1].t, l.events[1].dur), (200, 200));
    }

    #[test]
    fn events_store_the_pitches_and_sound_they_were_played_with() {
        let mut l = looper(1);
        l.pedal(0);
        l.note_on(0, 1, c(67), 4, 7, true);
        l.note_off(200, 1);
        run(&mut l, 0, 801);
        let e = l.events[0];
        assert_eq!((e.chord, e.patch, e.arp), (c(67), 7, true));
        assert_eq!(l.layer(0), Some((Layer { patch: 7, arp: true, muted: false }, false)));
    }

    #[test]
    fn playback_lands_on_the_exact_recorded_sample_every_cycle() {
        let mut l = one_bar_loop(); // playing from sample 800
        let fired = run(&mut l, 801, 800 + BAR * 3);
        assert_eq!(fired, vec![(1200, 4), (1600, 0), (2000, 4), (2400, 0), (2800, 4)]);
    }

    #[test]
    fn a_chord_held_when_recording_ends_ends_with_it() {
        let mut l = looper(1);
        l.pedal(0);
        play(&mut l, 0, 100, 1, 0);
        l.note_on(400, 2, c(67), 4, 0, false);
        run(&mut l, 0, 801); // still held when the bar is up
        assert_eq!(l.events[1].dur, 400);
    }

    #[test]
    fn a_layer_is_one_pass_heard_from_the_next_time_round() {
        let mut l = one_bar_loop();
        run(&mut l, 801, 1300);
        l.pedal(1300); // -> overdub, at loop position 500
        assert_eq!(l.state, LoopState::Overdub);
        play(&mut l, 1400, 1500, 9, 3); // position 600

        // Not heard in the cycle it was played in...
        let rest = run(&mut l, 1300, 1600);
        assert!(rest.iter().all(|&(_, d)| d != 3), "layer played early: {rest:?}");

        // ...the pass ends by itself one loop after it began...
        run(&mut l, 1600, 2100);
        assert_eq!(l.state, LoopState::Overdub);
        run(&mut l, 2100, 2101);
        assert_eq!(l.state, LoopState::Playing);

        // ...and it plays in order, at its recorded position, from then on.
        let next = run(&mut l, 2400, 3200);
        assert_eq!(next, vec![(2400, 0), (2800, 4), (3000, 3)]);
        assert_eq!(l.layer(1).map(|(_, rec)| rec), Some(false));
    }

    #[test]
    fn a_pass_with_nothing_played_leaves_no_layer() {
        let mut l = one_bar_loop();
        l.pedal(900);
        run(&mut l, 900, 900 + BAR + 1);
        assert_eq!(l.state, LoopState::Playing);
        assert_eq!(l.layer(1), None);
    }

    #[test]
    fn a_note_held_over_the_loop_end_is_cut_at_the_boundary() {
        let mut l = one_bar_loop(); // wraps every 800 from 800
        run(&mut l, 801, 1400);
        l.pedal(1400); // overdub at position 600
        l.note_on(1400, 9, c(65), 3, 0, false);
        run(&mut l, 1400, 1700); // wraps at 1600
        l.note_off(1700, 9); // position 100, after the wrap
        run(&mut l, 1700, 2401); // pass ends at 2200, merged at the wrap at 2400
        let ev = l.events.iter().find(|e| e.degree == 3).expect("layer kept");
        assert_eq!(ev.t, 600);
        assert_eq!(ev.dur, 200, "should stop at the loop end, 800 - 600");
    }

    #[test]
    fn undo_takes_back_the_newest_layer_only() {
        let mut l = one_bar_loop();
        l.pedal(800);
        play(&mut l, 900, 1000, 9, 3);
        run(&mut l, 801, 1601);
        assert!(l.layer(1).is_some());

        l.undo();
        assert_eq!(l.layer(1), None);
        assert_eq!(l.state, LoopState::Playing, "the first layer should keep playing");
        assert_eq!(run(&mut l, 1601, 2400), vec![(2000, 4)]);
        assert!(l.events.iter().all(|e| e.degree != 3));
    }

    #[test]
    fn undo_while_recording_a_layer_throws_that_pass_away() {
        let mut l = one_bar_loop();
        l.pedal(900);
        l.note_on(1000, 9, c(65), 3, 0, false); // still held
        l.undo();
        assert_eq!(l.state, LoopState::Playing);
        l.note_off(1100, 9); // the finger lifts later: nothing to record into
        run(&mut l, 801, 3000);
        assert_eq!(l.event_count(), 2);
        assert!(l.layer(0).is_some(), "the loop underneath was lost");
    }

    #[test]
    fn undoing_the_only_layer_clears_the_loop() {
        let mut l = one_bar_loop();
        l.undo();
        assert_eq!(l.state, LoopState::Idle);
        assert!(run(&mut l, 801, 5000).is_empty());
    }

    #[test]
    fn deleting_a_middle_layer_keeps_the_ones_above_it() {
        let mut l = one_bar_loop(); // layer 0: pads 0 and 4
        run(&mut l, 801, 1600);
        for (i, degree, start) in [(1u64, 1u8, 1600u64), (2, 2, 2401)] {
            l.pedal(start);
            l.note_on(start + 100, 10 + i, c(62), degree, i as u8, false);
            l.note_off(start + 200, 10 + i);
            run(&mut l, start, start + BAR + 1); // the pass ends on the last tick
        }
        assert_eq!(l.layer(2).map(|(layer, _)| layer.patch), Some(2));

        l.remove_layer(1);
        assert_eq!(l.layer(1).map(|(layer, _)| layer.patch), Some(2), "layer 2 moved down");
        assert_eq!(l.layer(2), None);
        run(&mut l, 3202, 4000);
        let fired: Vec<u8> = run(&mut l, 4000, 4800).iter().map(|x| x.1).collect();
        assert_eq!(fired, vec![0, 2, 4]);
    }

    #[test]
    fn a_muted_layer_is_silent_until_unmuted() {
        let mut l = one_bar_loop();
        l.toggle_mute(0);
        assert!(run(&mut l, 801, 2400).is_empty());
        l.toggle_mute(0);
        assert_eq!(run(&mut l, 2400, 3200), vec![(2400, 0), (2800, 4)]);
    }

    #[test]
    fn layers_stop_at_the_limit() {
        let mut l = one_bar_loop();
        let mut now = 800;
        for i in 1..MAX_LAYERS as u64 + 3 {
            l.pedal(now);
            l.note_on(now, 100 + i, c(60), 1, 0, false);
            l.note_off(now + 100, 100 + i);
            run(&mut l, now, now + BAR + 1);
            now += BAR + 1;
        }
        assert!(l.layer(MAX_LAYERS - 1).is_some());
        assert_eq!(l.layer_count, MAX_LAYERS);
        l.pedal(now);
        assert_eq!(l.state, LoopState::Playing, "a ninth layer started recording");
    }

    #[test]
    fn the_click_counts_beats_while_recording_and_accents_each_bar() {
        let mut l = looper(1);
        l.pedal(0);
        l.note_on(0, 1, c(60), 0, 0, false);
        let mut beats = vec![];
        for now in 0..BAR + 400 {
            l.tick(now, |_| {});
            if let Some(accent) = l.beat(now) {
                beats.push((now, accent));
            }
        }
        // The loop closed at 800, and a playing loop does not click.
        assert_eq!(beats, vec![(0, true), (200, false), (400, false), (600, false)]);
    }

    #[test]
    fn tempo_is_locked_once_a_loop_exists() {
        let mut l = one_bar_loop();
        l.set_step(50);
        assert_eq!(l.loop_bars(), 1);
        l.clear();
        l.set_step(50);
        assert_eq!(l.bar_len(), 400);
    }

    #[test]
    fn stop_silences_and_play_restarts_from_the_top() {
        let mut l = one_bar_loop();
        run(&mut l, 801, 1400);
        l.play_stop(1400);
        assert_eq!(l.state, LoopState::Stopped);
        assert!(run(&mut l, 1400, 5000).is_empty(), "a stopped loop played");

        l.play_stop(5000);
        assert_eq!(run(&mut l, 5000, 5800), vec![(5000, 0), (5400, 4)]);
    }

    #[test]
    fn clear_forgets_everything() {
        let mut l = one_bar_loop();
        l.clear();
        assert_eq!(l.state, LoopState::Idle);
        assert_eq!(l.event_count(), 0);
        assert_eq!(l.layer(0), None);
        assert!(run(&mut l, 0, 5000).is_empty());
    }

    #[test]
    fn position_runs_zero_to_one_round_the_loop_and_through_the_recording() {
        let mut l = looper(1);
        l.pedal(0);
        l.note_on(0, 1, c(60), 0, 0, false);
        assert!((l.position(400) - 0.5).abs() < 1e-6, "recording progress");
        let l = one_bar_loop();
        assert_eq!(l.position(800), 0.0);
        assert!((l.position(1200) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn recording_never_grows_past_the_preallocated_storage() {
        // Growing the Vec would allocate on the audio thread. Hammer it with
        // far more chords than fit and check the allocation never moved.
        let mut l = Looper::new(1, 1);
        l.bars = 8;
        let cap = l.events.capacity();
        l.pedal(0);
        for i in 0..(MAX_EVENTS as u64 * 2) {
            l.note_on(i * 3, i, c(60), 0, 0, false);
            l.note_off(i * 3 + 1, i);
        }
        assert_eq!(l.events.len(), MAX_EVENTS);
        assert_eq!(l.events.capacity(), cap, "events reallocated");
    }
}

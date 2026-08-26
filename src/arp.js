/*
 * arp.js - the arpeggiator. Instead of a chord sounding as a block, its notes
 * take turns, one per beat, for as long as the chord is held.
 *
 * This is the feature that makes the instrument sound like finished music
 * rather than someone leaning on an organ, and it is worth understanding why:
 * a held triad is three notes at once, which is a texture. The same three notes
 * one after another is a *pattern*, and a pattern has rhythm. Nothing about the
 * harmony changes - it is the same three pitches either way.
 *
 * ---------------------------------------------------------------------------
 * ONE IDEA: A HOLD
 * ---------------------------------------------------------------------------
 *
 * Chords arrive here from two completely different places:
 *
 *   - your finger, which starts a chord now and ends it at some unknown
 *     future moment when you let go
 *   - the looper, which knows in advance that chord 4 starts at t=12.80 and
 *     ends at t=13.55
 *
 * Rather than write the arpeggiator twice, both are expressed as the same
 * thing: a HOLD, meaning "this chord is down over this span of time".
 *
 *     { notes, from, until }
 *
 * A live hold simply has `until = Infinity` until you release it. That one
 * shared idea is why a loop you recorded as block chords starts arpeggiating
 * the moment you switch the arpeggiator on: the looper hands over a span of
 * time and a set of pitches, and so does your finger.
 *
 * A hold carries pitches rather than a chord number on purpose. A recorded
 * chord keeps the pitches it was played with, so an arpeggiated loop stays in
 * the key it was recorded in even after you change key to play over it.
 *
 * ---------------------------------------------------------------------------
 * THE CLOCK
 * ---------------------------------------------------------------------------
 *
 * Same two-clock pattern as looper.js, and for the same reason: a sloppy JS
 * timer wakes up often and decides what to schedule, while every note is
 * stamped with an exact time on the audio hardware's clock. See the long
 * comment at the top of looper.js - the reasoning is identical and is not
 * repeated here.
 *
 * The arpeggiator keeps its own step grid rather than sharing the looper's
 * scheduler. They look similar but they are not the same clock: the looper's
 * position wraps at the end of a loop, while the arp grid free-runs forever at
 * whatever tempo you dial in. Fusing them would mean one clock pretending to be
 * two, which is more code than having two.
 */

const ARP_TICK_MS = 25;      // how often the sloppy timer wakes
const ARP_AHEAD = 0.12;      // how far ahead of the audio clock we queue notes
const ARP_STORE = 'heptad.arp';

// A step is an eighth note - two per beat. Sixteenths are frantic at any
// reasonable tempo and quarter notes barely register as an arpeggio.
const STEPS_PER_BEAT = 2;

let arpOn = false;
let arpPattern = 'up';
let arpBpm = 120;

let holds = [];        // {notes, from, until, step}
let nextStep = 0;      // audio-clock time of the next step on the grid
let arpTimer = null;

/** Seconds per step, straight from the tempo. */
function stepDur() {
  return 60 / arpBpm / STEPS_PER_BEAT;
}

/* ------------------------------------------------------------------ holds -- */

/**
 * A chord went down under your finger. It rings until arpHoldOff, which may be
 * in a tenth of a second or may be in a minute - we don't know and don't care.
 */
function arpHoldOn(id, degree) {
  // Resolved now, against the key that is current now. Changing key releases
  // everything held anyway, so a live hold can never outlive its own chord.
  holds.push({ id, notes: [...chords[degree]], from: ctx.currentTime, until: Infinity, step: 0 });
  startArpClock();
}

function arpHoldOff(id) {
  // Ending the hold rather than deleting it: steps for the span you *did* hold
  // may already be scheduled, and they should still play. The tick drops it.
  for (const hold of holds) {
    if (hold.id === id) hold.until = ctx.currentTime;
  }
}

/**
 * A chord the looper already knows the full shape of, in the pitches it was
 * recorded with. No id, because nothing will ever come looking for it - it
 * expires on its own at `until`.
 */
function arpScheduleHold(notes, from, until) {
  holds.push({ id: null, notes, from, until, step: 0 });
  startArpClock();
}

/* -------------------------------------------------------------- the clock -- */

function startArpClock() {
  if (arpTimer || !ctx) return;
  nextStep = ctx.currentTime;
  arpTimer = setInterval(arpTick, ARP_TICK_MS);
}

function stopArpClock() {
  clearInterval(arpTimer);
  arpTimer = null;
  holds = [];
}

function arpTick() {
  if (!ctx) return;
  const now = ctx.currentTime;
  const horizon = now + ARP_AHEAD;

  // A backgrounded tab throttles this timer to about once a second while the
  // audio clock keeps running. Without this the grid would be far in the past
  // on return and we'd flush every missed step at once - a burst of noise.
  // Skip the grid forward to now instead; you lose the steps you couldn't have
  // heard anyway.
  if (nextStep < now) nextStep = now;

  while (nextStep < horizon) {
    for (const hold of holds) {
      if (nextStep >= hold.from && nextStep < hold.until) playStep(hold, nextStep);
    }
    nextStep += stepDur();
  }

  // Retire holds that have finished. Live holds sit at Infinity until released,
  // so this only ever collects looped ones and ones you've let go of.
  holds = holds.filter((hold) => hold.until > now);
}

/** Play one note of one hold's chord, at an exact time on the audio clock. */
function playStep(hold, at) {
  const order = arpSequence(arpPattern, hold.notes.length);

  // `step` counts from the moment this chord went down, not from some global
  // beat - so every chord you play starts its pattern on its own first note.
  const midi = hold.notes[order[hold.step % order.length]];
  hold.step++;

  // Slightly shorter than a step so consecutive notes separate audibly instead
  // of running together into one continuous tone.
  const dur = Math.max(stepDur() * 0.8, ATTACK * 2);
  stopChord([startVoice(midi, at)], at + dur);
}

/* --------------------------------------------------------------- controls -- */

const arpBtn = document.getElementById('arp');
const arpPatternSel = document.getElementById('arppattern');
const arpTempoInput = document.getElementById('arptempo');
const arpTempoOut = document.getElementById('arptempoval');

function setArp(on) {
  arpOn = on;

  // Anything currently down was started as one kind of sound and would end as
  // the other, so let it all go. Cleaner than trying to convert a ringing block
  // chord into an arpeggio mid-note.
  releaseAll();
  if (!on) stopArpClock();

  arpBtn.classList.toggle('active', on);
  arpBtn.setAttribute('aria-pressed', String(on));
  saveArp();
}

function setTempo(bpm) {
  arpBpm = bpm;
  arpTempoOut.textContent = String(bpm);
  saveArp();
}

function saveArp() {
  try {
    localStorage.setItem(ARP_STORE,
      JSON.stringify({ on: arpOn, pattern: arpPattern, bpm: arpBpm }));
  } catch (_) { /* not worth interrupting playing over */ }
}

arpBtn.addEventListener('click', () => {
  if (bindMode) return; // in bind mode this button is being rebound, not pressed
  setArp(!arpOn);
  arpBtn.blur(); // or the spacebar would keep re-triggering it
});

arpPatternSel.addEventListener('change', () => {
  arpPattern = arpPatternSel.value;
  saveArp();
  arpPatternSel.blur();
});

// 'input' rather than 'change' so the tempo moves under your finger as you drag
// it, instead of jumping when you let go.
arpTempoInput.addEventListener('input', () => {
  setTempo(Number(arpTempoInput.value));
});
arpTempoInput.addEventListener('change', () => arpTempoInput.blur());

(function loadArp() {
  let saved = null;
  try {
    saved = JSON.parse(localStorage.getItem(ARP_STORE));
  } catch (_) { /* fall through to the defaults */ }

  if (saved) {
    // Clamp rather than trust: the slider's own min/max are the real contract,
    // and a bad stored tempo could otherwise divide by zero or freeze the grid.
    const bpm = Number(saved.bpm);
    if (Number.isFinite(bpm)) {
      arpBpm = Math.min(Number(arpTempoInput.max), Math.max(Number(arpTempoInput.min), bpm));
    }
    if (typeof saved.pattern === 'string') arpPattern = saved.pattern;
    arpOn = !!saved.on;
  }

  arpTempoInput.value = String(arpBpm);
  arpPatternSel.value = arpPattern;
  setTempo(arpBpm);
  arpBtn.classList.toggle('active', arpOn);
  arpBtn.setAttribute('aria-pressed', String(arpOn));
})();

/*
 * looper.js - record what you play, loop it, and stack more on top.
 *
 * Works like a guitarist's loop pedal:
 *
 *   record  ->  play a phrase  ->  close loop  ->  it repeats forever
 *                                                  ->  overdub more onto it
 *
 * WHAT GETS RECORDED IS NOT AUDIO. We store what you *did* - "chord 4 started
 * 1.2 seconds in and lasted 0.8 seconds" - and re-perform it on playback. That
 * takes a few hundred bytes instead of megabytes, never degrades no matter how
 * many times you overdub, and means a recorded loop will automatically pick up
 * any future change to the voice, the key, or the arpeggiator. Recording actual
 * audio would freeze the sound as it was the moment you played it.
 *
 * ---------------------------------------------------------------------------
 * THE CLOCK PROBLEM, which is the whole reason this file is more than 20 lines
 * ---------------------------------------------------------------------------
 *
 * The obvious way to replay a loop is setInterval: wake up, play the note, go
 * back to sleep. It does not work. setInterval is scheduled by the JavaScript
 * event loop, which is at the mercy of layout, garbage collection and whatever
 * else the page is doing, so it fires late by a wobbling handful of
 * milliseconds. Rhythm is exactly the thing humans notice being a few
 * milliseconds off, and the errors accumulate.
 *
 * Web Audio has its own clock - `ctx.currentTime` - running on the audio
 * hardware, and every scheduling method (`osc.start(t)`, `gain.setValueAtTime`)
 * takes a time on that clock and hits it exactly. So we use two clocks:
 *
 *   - a sloppy JS timer that wakes up often (every 25ms) and does no timing
 *   - the sample-accurate audio clock, which everything is actually scheduled
 *     against
 *
 * Each time the sloppy timer wakes, it looks 120ms into the future and hands
 * the audio hardware every note that falls in that window, stamped with the
 * exact time it should sound. The timer can wobble by tens of milliseconds and
 * nothing changes, because it is only deciding *when to think ahead*, never
 * *when to make a sound*. This is the pattern Chris Wilson wrote up as
 * "A Tale of Two Clocks"; it is the standard answer for audio timing on the web.
 */

const TICK_MS = 25;          // how often the sloppy timer wakes up
const SCHEDULE_AHEAD = 0.12; // how far ahead of the audio clock we queue notes

/* ------------------------------------------------------------------ state --

   loopState is a small machine:

     idle ---record---> armed ---first chord---> recording
                                                     |
                                                  close loop
                                                     v
     stopped <---stop--- playing <---end overdub--- playing
        |                  |  ^                       (looping)
      play               overdub |
        `------------------'-----'                                            */

let loopState = 'idle';

let loopEvents = [];   // {t, degree, dur} - the loop itself, sorted by t
let loopPending = [];  // recorded during this overdub pass, merged at the wrap
let openNotes = new Map(); // input id -> {degree, start} for chords still held

let loopLength = 0;    // seconds; fixed when you close the loop
let recStart = 0;      // audio-clock time the first pass began
let playStart = 0;     // audio-clock time of position 0 of the first cycle

// Where the scheduler has got to. It runs ahead of what you can hear.
let nextIndex = 0;
let nextCycle = 0;

let timer = null;
let sounding = []; // {voices, endsAt} the loop has started but not yet finished

/** True while the loop is cycling, whether or not it's also recording. */
function isLoopRunning() {
  return loopState === 'playing' || loopState === 'overdub';
}

/**
 * How far into the loop we are, right now, in seconds.
 *
 * During the first pass the loop has no length yet, so this just counts up.
 * Afterwards it wraps - and note it is measured against the *real* audio clock,
 * not the scheduler's position, which is deliberately ahead of it.
 */
function loopPhase() {
  if (loopState === 'recording') return ctx.currentTime - recStart;
  if (!loopLength) return 0;
  return (ctx.currentTime - playStart) % loopLength;
}

/* -------------------------------------------------------------- recording --

   app.js calls these on every chord you play, whether from a pad or a key.
   They do nothing unless we're actually recording, so there is no "is the
   looper on?" check scattered through the input code.                        */

function loopNoteOn(id, degree) {
  if (loopState === 'armed') {
    // The loop starts on your first chord, not when you hit the button - so
    // there's no silent gap at the front of the loop while you get ready.
    loopState = 'recording';
    recStart = ctx.currentTime;
    updateTransport();
  }
  if (loopState !== 'recording' && loopState !== 'overdub') return;
  openNotes.set(id, { degree, start: loopPhase() });
}

function loopNoteOff(id) {
  const note = openNotes.get(id);
  if (!note) return;
  openNotes.delete(id);

  let dur = loopPhase() - note.start;

  // A chord held across the end of the loop comes back round as a negative
  // duration. Cut it off at the loop boundary rather than trying to wrap it -
  // a note that spills into the next cycle would fight with the copy of itself
  // starting there.
  // ponytail: truncate rather than wrap. If held-over notes ever matter
  // musically, split them into a tail event at t=0.
  if (dur < 0) dur = loopLength - note.start;

  // Never shorter than the attack, so a stab still gets its full fade-in and
  // scheduled playback can assume the envelope has finished rising.
  dur = Math.max(dur, ATTACK);

  const event = { t: note.start, degree: note.degree, dur };
  if (loopState === 'recording') loopEvents.push(event);
  else loopPending.push(event);
}

/** Close off anything still held - used when a pass ends mid-chord. */
function closeOpenNotes() {
  [...openNotes.keys()].forEach(loopNoteOff);
}

/* -------------------------------------------------------------- scheduler -- */

function tick() {
  const horizon = ctx.currentTime + SCHEDULE_AHEAD;

  // Drop the bookkeeping for loop notes that have already finished sounding.
  sounding = sounding.filter((s) => s.endsAt > ctx.currentTime);

  // Browsers throttle setInterval to roughly once a second in a background
  // tab, which starves the scheduler while the audio clock keeps running. Come
  // back to the tab and it would otherwise try to catch up by flushing every
  // missed note at once - a burst of noise. Skip forward to the cycle we are
  // actually in instead, and pick up from there.
  const behind = ctx.currentTime - (playStart + nextCycle * loopLength);
  if (behind > loopLength) {
    nextCycle = Math.floor((ctx.currentTime - playStart) / loopLength);
    nextIndex = 0;
  }

  // `guard` is pure paranoia: a loop length of ~0 would otherwise spin here
  // forever and hang the tab.
  let guard = 0;
  while (guard++ < 1000) {
    if (nextIndex >= loopEvents.length) {
      // Reached the end of a cycle. Only roll over once the horizon has
      // actually passed the boundary, or we'd queue up cycles forever.
      const cycleEnd = playStart + (nextCycle + 1) * loopLength;
      if (cycleEnd >= horizon) break;

      // The wrap is the clean moment to fold in an overdub: index is about to
      // reset to zero, so a re-sorted array can't confuse our position.
      // ponytail: anything played inside the last 120ms of a pass joins on the
      // following cycle instead. Cheap fix if it ever annoys: merge at the
      // audible boundary rather than the scheduling one.
      if (loopPending.length) {
        loopEvents = [...loopEvents, ...loopPending].sort((a, b) => a.t - b.t);
        loopPending = [];
      }

      nextIndex = 0;
      nextCycle++;
      continue;
    }

    const event = loopEvents[nextIndex];
    const at = playStart + nextCycle * loopLength + event.t;
    if (at >= horizon) break; // too far out; we'll catch it on a later tick

    // Its moment has already gone (see the throttling note above). Let it go
    // rather than playing it late and out of time.
    if (at >= ctx.currentTime) playEvent(event, at);
    nextIndex++;
  }

  drawLoopBar();
}

/** Hand one recorded chord to the audio hardware, stamped with its exact time. */
function playEvent(event, at) {
  const voices = startChord(event.degree, at);
  stopChord(voices, at + event.dur);
  sounding.push({ voices, endsAt: at + event.dur + RELEASE * 8 });
  flashPad(event.degree, at, event.dur);
}

/**
 * Light a pad in time with a note the loop is playing.
 *
 * Visuals don't need the audio clock's precision, so a plain setTimeout aimed
 * at the right moment is fine - being a frame late is invisible, whereas being
 * a frame late with a note is audible.
 */
function flashPad(degree, at, dur) {
  const delay = Math.max(0, (at - ctx.currentTime) * 1000);
  setTimeout(() => {
    const pad = pads[degree];
    pad.dataset.loopCount = Number(pad.dataset.loopCount || 0) + 1;
    pad.classList.add('looping');
    setTimeout(() => {
      const left = Number(pad.dataset.loopCount) - 1;
      pad.dataset.loopCount = left;
      if (left <= 0) pad.classList.remove('looping');
    }, dur * 1000);
  }, delay);
}

function startScheduler() {
  if (timer === null) timer = setInterval(tick, TICK_MS);
}

function stopScheduler() {
  clearInterval(timer);
  timer = null;

  // Silence anything the loop had already queued up. Without this, notes
  // scheduled into the next 120ms keep sounding after you press stop.
  sounding.forEach((s) => stopChord(s.voices));
  sounding = [];
  pads.forEach((pad) => {
    pad.classList.remove('looping');
    pad.dataset.loopCount = 0;
  });
  drawLoopBar();
}

/* -------------------------------------------------------------- transport -- */

/**
 * The pedal. One button that means the obvious next thing, which is how loop
 * pedals work and why they're playable with a foot.
 */
function pedal() {
  ensureAudio();

  switch (loopState) {
    case 'idle':
      loopState = 'armed'; // starts for real on your first chord
      break;

    case 'armed': // pressed again before playing anything - never mind
      loopState = 'idle';
      break;

    case 'recording': {
      // Closing the loop: its length is however long you just played for.
      closeOpenNotes();
      loopLength = ctx.currentTime - recStart;
      loopEvents.sort((a, b) => a.t - b.t);
      playStart = ctx.currentTime; // cycle 0 begins the instant you close it
      nextIndex = 0;
      nextCycle = 0;
      loopState = 'playing';
      startScheduler();
      break;
    }

    case 'playing':
      loopState = 'overdub';
      break;

    case 'overdub':
      closeOpenNotes();
      loopState = 'playing';
      break;

    case 'stopped':
      // Overdubbing from a stop restarts the loop from the top.
      restart();
      loopState = 'overdub';
      break;
  }
  updateTransport();
}

function restart() {
  playStart = ctx.currentTime;
  nextIndex = 0;
  nextCycle = 0;
  startScheduler();
}

function togglePlay() {
  ensureAudio();
  if (isLoopRunning()) {
    closeOpenNotes();
    stopScheduler();
    loopState = 'stopped';
  } else if (loopState === 'stopped') {
    restart();
    loopState = 'playing';
  }
  updateTransport();
}

function clearLoop() {
  stopScheduler();
  loopEvents = [];
  loopPending = [];
  openNotes.clear();
  loopLength = 0;
  loopState = 'idle';
  updateTransport();
}

/* --------------------------------------------------------------------- ui -- */

const recBtn = document.getElementById('rec');
const playBtn = document.getElementById('play');
const clearBtn = document.getElementById('clear');
const loopFill = document.getElementById('loopfill');

// What the pedal button says in each state - it always names what pressing it
// will do next, never what the looper is currently doing.
const PEDAL_LABEL = {
  idle: 'record',
  armed: 'play a chord to start',
  recording: 'close loop',
  playing: 'overdub',
  overdub: 'end overdub',
  stopped: 'overdub',
};

function updateTransport() {
  const running = isLoopRunning();
  const hasLoop = loopLength > 0;

  // Only the label span - the key hint beside it belongs to app.js.
  recBtn.querySelector('.label').textContent = PEDAL_LABEL[loopState];
  recBtn.classList.toggle(
    'armed',
    loopState === 'armed' || loopState === 'recording' || loopState === 'overdub'
  );

  playBtn.querySelector('.label').textContent = running ? 'stop' : 'play';
  playBtn.disabled = !hasLoop;
  clearBtn.disabled = loopState === 'idle';

  drawLoopBar();
}

/**
 * The progress bar under the pads. Worth having: overdubbing in time is much
 * easier when you can see where the loop is about to come round again.
 */
function drawLoopBar() {
  const running = isLoopRunning();
  loopFill.style.width = running ? `${(loopPhase() / loopLength) * 100}%` : '0%';
  loopFill.classList.toggle('overdub', loopState === 'overdub');
}

recBtn.addEventListener('click', () => { pedal(); recBtn.blur(); });
playBtn.addEventListener('click', () => { togglePlay(); playBtn.blur(); });
clearBtn.addEventListener('click', () => { clearLoop(); clearBtn.blur(); });

updateTransport();

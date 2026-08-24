/*
 * app.js - the instrument. Everything that isn't pure theory lives here:
 * building the pads, listening for fingers, and making sound.
 *
 * Loaded as a plain script after theory.js, so chordsInKey, midiToFreq and
 * friends are already available as globals.
 */

/* ---------------------------------------------------------------- the key --

   Milestone 1 is C major and nothing else. MIDI 60 is middle C, which puts the
   seven chords between middle C and the F above it - a comfortable, unshrill
   register. The key and mode become selectable in milestone 3; when they do,
   only these two lines change, because the theory layer never assumed C.      */

const KEY_ROOT = 60;
const CHORDS = chordsInKey(KEY_ROOT, MAJOR_SCALE);

/* --------------------------------------------------------------- the voice --

   One oscillator per note, three notes per chord, up to seven chords held at
   once - so the mixer has to survive 21 simultaneous square waves.            */

const WAVE = 'square';       // buzzy and chiptune-ish, as opposed to 'sine'
const VOICE_GAIN = 0.15;     // level of a single note before the master stage
const ATTACK = 0.012;        // seconds to fade a note in
const RELEASE = 0.09;        // release time constant, in seconds

let ctx = null;      // the AudioContext, created on first touch (see below)
let master = null;   // every voice connects here

/*
 * TRAP: iOS (and, less silently, desktop Chrome) refuses to start audio unless
 * the AudioContext is created or resumed inside a real user gesture. Get this
 * wrong and there is no error at all - the page just stays mute forever. So we
 * build the context lazily on the first pointerdown/keydown and re-resume it
 * every time, because the OS can suspend it again later (a phone call, the
 * screen locking, the tab going to the background).
 */
function ensureAudio() {
  if (!ctx) {
    ctx = new (window.AudioContext || window.webkitAudioContext)();

    master = ctx.createGain();
    master.gain.value = 0.5;

    // Square waves are all sharp corners, and sharp corners are high harmonics.
    // Stacked 21 deep that reads as a hiss more than a chord, so shave the very
    // top off. Adjustable in milestone 4; fixed for now.
    const tone = ctx.createBiquadFilter();
    tone.type = 'lowpass';
    tone.frequency.value = 2600;
    tone.Q.value = 0.7;

    // Headroom insurance. Two hands down at once is far more signal than one
    // chord, and anything over 1.0 clips into a nasty crackle. The compressor
    // leans on the loud moments instead.
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -10;
    limiter.knee.value = 12;
    limiter.ratio.value = 12;
    limiter.attack.value = 0.003;
    limiter.release.value = 0.25;

    master.connect(tone).connect(limiter).connect(ctx.destination);
  }
  if (ctx.state !== 'running') ctx.resume();
}

/**
 * Start every note of a chord and return the running voices, so whoever is
 * holding the pad can hand them back to stopChord later.
 */
function startChord(degree) {
  const now = ctx.currentTime;

  return CHORDS[degree].map((midi) => {
    const osc = ctx.createOscillator();
    osc.type = WAVE;
    osc.frequency.value = midiToFreq(midi);

    const env = ctx.createGain();

    // TRAP: jumping gain straight to full volume steps the waveform
    // discontinuously, and a discontinuity is literally a click. Ramp instead -
    // 12ms is short enough to feel instant and long enough to be silent.
    env.gain.setValueAtTime(0, now);
    env.gain.linearRampToValueAtTime(VOICE_GAIN, now + ATTACK);

    osc.connect(env).connect(master);
    osc.start(now);
    return { osc, env };
  });
}

/** Fade a chord out and free its oscillators. */
function stopChord(voices) {
  const now = ctx.currentTime;

  for (const { osc, env } of voices) {
    // If the pad was stabbed, the attack ramp may still be running. Cancel it,
    // pin the gain to wherever it actually got to, and fall from there -
    // otherwise the value jumps and clicks on the way out too.
    env.gain.cancelScheduledValues(now);
    env.gain.setValueAtTime(env.gain.value, now);

    // setTargetAtTime is an exponential fall: natural-sounding, and it never
    // quite reaches zero, so stop the oscillator once it's inaudible (about
    // eight time constants down) rather than waiting forever.
    env.gain.setTargetAtTime(0, now, RELEASE);
    osc.stop(now + RELEASE * 8);
  }
}

/* ------------------------------------------------------------------- pads -- */

const padsEl = document.getElementById('pads');
const statusEl = document.getElementById('status');

const pads = CHORDS.map((notes, degree) => {
  const pad = document.createElement('button');
  pad.className = 'pad';
  pad.type = 'button';

  // One hue per degree, spread evenly round the colour wheel. Purely so the
  // pads are told apart at a glance while playing.
  pad.style.setProperty('--h', Math.round((degree * 360) / 7));

  const numeral = romanNumeral(notes, degree);
  pad.innerHTML =
    `<span class="numeral">${numeral}</span>` +
    `<span class="chord">${chordName(notes)}</span>` +
    `<span class="notes">${notes.map(noteName).join(' ')}</span>`;
  pad.setAttribute('aria-label', `Chord ${degree + 1}, ${chordName(notes)}`);

  padsEl.appendChild(pad);
  return pad;
});

/* ------------------------------------------------------------------ input --

   `held` maps an input source to the chord it is currently sounding. Each
   finger has its own pointerId and each key its own id, which is what makes
   the instrument polyphonic and multi-touch: seven independent holders, no
   shared "current chord" variable to fight over.                              */

const held = new Map(); // id -> { pad, voices }

function press(id, degree) {
  if (held.has(id)) return; // already sounding from this finger/key
  ensureAudio();
  const pad = pads[degree];
  held.set(id, { pad, voices: startChord(degree) });
  pad.classList.add('on');
}

function release(id) {
  const holding = held.get(id);
  if (!holding) return;
  held.delete(id);
  stopChord(holding.voices);

  // Two fingers can sit on the same pad; only unlight it when the last one goes.
  const stillHeld = [...held.values()].some((h) => h.pad === holding.pad);
  if (!stillHeld) holding.pad.classList.remove('on');
}

pads.forEach((pad, degree) => {
  pad.addEventListener('pointerdown', (e) => {
    e.preventDefault(); // stop the browser turning the press into a scroll

    // Capture routes pointerup to this pad even if the finger drifts off it
    // mid-hold, which otherwise leaves the note ringing forever.
    pad.setPointerCapture(e.pointerId);
    press(e.pointerId, degree);
  });

  const up = (e) => release(e.pointerId);
  pad.addEventListener('pointerup', up);
  pad.addEventListener('pointercancel', up); // finger stolen by a system gesture
});

// Number keys 1-7, so the thing is playable at a laptop without a touchscreen.
addEventListener('keydown', (e) => {
  const degree = '1234567'.indexOf(e.key);
  if (degree === -1 || e.repeat) return;
  press('key' + degree, degree);
});
addEventListener('keyup', (e) => {
  const degree = '1234567'.indexOf(e.key);
  if (degree !== -1) release('key' + degree);
});

// Stuck-note insurance: if the page loses focus mid-hold the matching release
// event never arrives, and the chord would drone on behind whatever you
// switched to.
const releaseAll = () => [...held.keys()].forEach(release);
addEventListener('blur', releaseAll);
addEventListener('visibilitychange', () => document.hidden && releaseAll());

// Long-press on a button pops the text-selection menu on mobile. Not here.
addEventListener('contextmenu', (e) => e.preventDefault());

// Silence any browser that ignores the meta-viewport zoom lock.
addEventListener('gesturestart', (e) => e.preventDefault());

// Confirm the audio actually woke up. If it doesn't, the status line says so,
// which beats standing there wondering whether the phone is on silent.
padsEl.addEventListener('pointerdown', () => {
  setTimeout(() => {
    if (ctx && ctx.state !== 'running') {
      statusEl.innerHTML = 'audio blocked &mdash; tap again, or check the silent switch';
    }
  }, 250);
}, { once: false });

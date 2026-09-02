/*
 * app.js - the instrument. Everything that isn't pure theory lives here:
 * building the pads, listening for fingers, and making sound.
 *
 * Loaded as a plain script after theory.js, so chordsInKey, midiToFreq and
 * friends are already available as globals.
 */

/* ---------------------------------------------------------------- the key --

   All twelve keys, major or minor. The theory layer never assumed C, so all
   this does is pick a root note and a scale pattern and ask it for the chords -
   there is no per-key chord table anywhere, and adding a third mode later would
   mean adding one array to theory.js and one <option> to the page.            */

const KEY_STORE = 'heptad.key';

let keyPitch = 0;        // 0-11, where 0 is C
let keyMode = 'major';
let keyRoot = 60;        // the MIDI note the chords are built from
let chords = [];         // the seven triads; rebuilt whenever the key changes

/**
 * Which octave a key starts in.
 *
 * Naively, B major would start a semitone below the C an octave up and put the
 * whole instrument in a shrill register. Keys past F# drop down an octave
 * instead of climbing, so every key sits within about a fifth of middle C.
 */
function rootMidiFor(pitch) {
  return 60 + (pitch > 6 ? pitch - 12 : pitch);
}

function setKey(pitch, mode) {
  keyPitch = pitch;
  keyMode = mode;
  keyRoot = rootMidiFor(pitch);

  // The one line that makes a mode a mode.
  const pattern = mode === 'minor' ? MINOR_SCALE : MAJOR_SCALE;
  chords = chordsInKey(keyRoot, pattern);

  releaseAll(); // don't leave the old key's chords ringing under the new one
  labelPads();
  setStatus(null);

  try {
    localStorage.setItem(KEY_STORE, JSON.stringify({ pitch, mode }));
  } catch (_) { /* not worth interrupting playing over */ }
}

function loadKey() {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY_STORE));
    if (saved && Number.isInteger(saved.pitch) && saved.pitch >= 0 && saved.pitch < 12) {
      return saved;
    }
  } catch (_) { /* fall through to C major */ }
  return { pitch: 0, mode: 'major' };
}

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
 *
 * `when` is a time on the audio clock. Live playing leaves it out and gets
 * "now"; the looper passes an exact future time so its notes land in rhythm.
 */
function startChord(degree, when) {
  return startNotes(chords[degree], when);
}

/**
 * Sound an explicit list of MIDI notes together.
 *
 * The looper plays through here rather than through startChord, because a
 * recorded chord carries the pitches it was played with - see loopNoteOff.
 */
function startNotes(notes, when) {
  // One `now` for the whole chord: read the clock per-note and the three notes
  // would start microseconds apart, which is a phasing artefact, not a chord.
  const now = when ?? ctx.currentTime;
  return notes.map((midi) => startVoice(midi, now));
}

/**
 * One oscillator, one envelope, running. The arpeggiator starts notes one at a
 * time rather than three at once, so this is split out of startChord.
 */
function startVoice(midi, when) {
  const now = when ?? ctx.currentTime;

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
}

/** Fade a chord out and free its oscillators, now or at a scheduled time. */
function stopChord(voices, when) {
  const now = ctx.currentTime;
  const at = when ?? now;

  for (const { osc, env } of voices) {
    if (at <= now + 0.001) {
      // Releasing right now. If the pad was stabbed the attack ramp may still
      // be running, so cancel it, pin the gain to wherever it actually got to,
      // and fall from there - otherwise the value jumps and clicks.
      env.gain.cancelScheduledValues(at);
      env.gain.setValueAtTime(env.gain.value, at);
    } else {
      // Releasing at a future time, which means we can't read the gain (it
      // hasn't happened yet). We don't need to: recorded durations are never
      // shorter than the attack, so by `at` the envelope is fully open.
      env.gain.setValueAtTime(VOICE_GAIN, at);
    }

    // setTargetAtTime is an exponential fall: natural-sounding, and it never
    // quite reaches zero, so stop the oscillator once it's inaudible (about
    // eight time constants down) rather than waiting forever.
    env.gain.setTargetAtTime(0, at, RELEASE);
    osc.stop(at + RELEASE * 8);
  }
}

/* --------------------------------------------------------------- bindings --

   Everything a key can be bound to: the seven pads (identified by their chord
   number) and the switches - looper transport and the arpeggiator - identified
   by name. One list, so every control shares the same rebinding machinery
   rather than having two or three of everything.

   Pads default to the home row, so the seven chords sit under your seven
   fingers with no reaching - that matters more than it sounds like it does once
   you're playing rather than poking. They answer to the number row as well,
   because the pads are labelled I..vii and sometimes the thing in your head is
   "chord five", not "the G key". Clear starts unbound on purpose: it wipes your
   loop, and that shouldn't be one stray keystroke away.                        */

const ACTIONS = [0, 1, 2, 3, 4, 5, 6, 'pedal', 'playstop', 'clear', 'arp'];

// Each action holds a LIST of keys, so one control can answer to more than one.
// The pads ship with two: the home row, which is what you play with, and the
// number row, which is what you reach for when you're thinking in chord numbers
// and the roman numerals on the pads are the thing in your head.
const DEFAULT_BINDINGS = [
  ['a', '1'], ['s', '2'], ['d', '3'], ['f', '4'], ['g', '5'], ['h', '6'], ['j', '7'],
  [' '], ['escape'], [], ['q'],
];
const BINDINGS_KEY = 'heptad.bindings';

let bindings = loadBindings();

function loadBindings() {
  try {
    const saved = JSON.parse(localStorage.getItem(BINDINGS_KEY));
    if (!Array.isArray(saved) || saved.length > ACTIONS.length) return freshBindings();

    // Two shapes have been saved over this project's life: one key per action
    // as a bare string, and the current list-per-action. Read both, so nobody
    // has to reset a layout they set up.
    const wasSingleKey = !saved.every((entry) => Array.isArray(entry));
    const merged = ACTIONS.map((_, i) => {
      const entry = saved[i];
      if (typeof entry === 'string') return entry ? [entry] : [];
      return Array.isArray(entry) ? entry.filter((k) => typeof k === 'string' && k) : [];
    });

    // Top up from the defaults, taking only keys nothing else is using - two
    // actions on one key means the later one silently wins.
    //
    // Which slots get topped up depends on where the save came from. An older
    // single-key save predates pads answering to two keys, so every action is
    // eligible and the number row arrives without anyone resetting anything. A
    // current save is taken at its word apart from genuinely new actions, or we
    // would resurrect keys the user had deliberately rebound away.
    const used = new Set(merged.flat());
    ACTIONS.forEach((_, i) => {
      if (!wasSingleKey && i < saved.length) return;
      for (const key of DEFAULT_BINDINGS[i]) {
        if (!used.has(key)) { merged[i].push(key); used.add(key); }
      }
    });
    return merged;
  } catch (_) { /* private browsing, disabled storage, corrupt JSON - ignore */ }
  return freshBindings();
}

/** A deep copy, so editing one pad's keys can't reach into the defaults. */
function freshBindings() {
  return DEFAULT_BINDINGS.map((keys) => [...keys]);
}

function saveBindings() {
  try {
    localStorage.setItem(BINDINGS_KEY, JSON.stringify(bindings));
  } catch (_) { /* not worth interrupting playing over */ }
}

/**
 * Keys are compared lowercased so shift doesn't matter, and space stays as the
 * single character the browser reports it as.
 */
function normalizeKey(key) {
  return key.toLowerCase();
}

/** How a single key is written on a pad or button. */
function keyLabel(key) {
  if (key === ' ') return 'space';
  if (key === 'escape') return 'esc';
  return key;
}

/** How an action's whole set of keys is written - "a/1", "space", "--". */
function keysLabel(keys) {
  if (!keys || !keys.length) return '--';
  return keys.map(keyLabel).join('/');
}

/** Reverse lookup, rebuilt whenever the bindings change: key -> action. */
let keyMap = new Map();
function rebuildKeyMap() {
  // Anything whose key was stolen by something else has an empty binding; leave
  // those out rather than letting every unbound action answer to the same "".
  keyMap = new Map();
  bindings.forEach((keys, i) => {
    for (const key of keys) keyMap.set(key, ACTIONS[i]);
  });
}
rebuildKeyMap();

/* ------------------------------------------------------------------- pads -- */

const padsEl = document.getElementById('pads');
const statusEl = document.getElementById('status');

/** Show a message in the top bar, or pass null to put the default line back. */
function setStatus(message) {
  statusEl.innerHTML = message
    ?? `HEPTAD &mdash; key of <b>${noteName(keyRoot)} ${keyMode}</b> &middot; hold a pad`;
}

const pads = Array.from({ length: MAJOR_SCALE.length }, (_, degree) => {
  const pad = document.createElement('button');
  pad.className = 'pad';
  pad.type = 'button';

  // One hue per degree, spread evenly round the colour wheel. Purely so the
  // pads are told apart at a glance while playing.
  pad.style.setProperty('--h', Math.round((degree * 360) / 7));

  // Filled in by labelPads(), which runs again every time the key changes.
  pad.innerHTML =
    '<span class="numeral"></span>' +
    '<span class="chord"></span>' +
    '<span class="notes"></span>' +
    '<span class="key"></span>';

  padsEl.appendChild(pad);
  return pad;
});

/** Write the current key's chords onto the pads. */
function labelPads() {
  chords.forEach((notes, degree) => {
    const pad = pads[degree];
    const name = chordName(notes);
    pad.querySelector('.numeral').textContent = romanNumeral(notes, degree);
    pad.querySelector('.chord').textContent = name;
    pad.querySelector('.notes').textContent = notes.map(noteName).join(' ');
    pad.setAttribute('aria-label', `Chord ${degree + 1}, ${name}`);
  });
}

/**
 * Print every bound key on the thing it triggers - pads on their faces,
 * transport keys on their buttons. This is how you learn them, and how you can
 * tell that space is the pedal without reading the README.
 */
function showBindings() {
  ACTIONS.forEach((action, i) => {
    const slot = typeof action === 'number'
      ? pads[action].querySelector('.key')
      : document.querySelector(`[data-action="${action}"] .hint`);
    slot.textContent = i === bindingIndex ? 'press a key' : keysLabel(bindings[i]);
  });
}

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

  // With the arpeggiator on the chord doesn't sound as a block: the arp takes
  // the hold and plays one note per step for as long as you keep holding, so
  // there are no block voices to hand back later.
  if (arpOn) arpHoldOn(id, degree);
  held.set(id, { pad, voices: arpOn ? null : startChord(degree) });

  pad.classList.add('on');
  loopNoteOn(id, degree); // no-op unless the looper is recording
}

function release(id) {
  const holding = held.get(id);
  if (!holding) return;
  held.delete(id);
  if (holding.voices) stopChord(holding.voices); // null while the arp had it
  arpHoldOff(id);                                // no-op if it never did
  loopNoteOff(id);

  // Two fingers can sit on the same pad; only unlight it when the last one goes.
  const stillHeld = [...held.values()].some((h) => h.pad === holding.pad);
  if (!stillHeld) holding.pad.classList.remove('on');
}

/* Rebinding state. `bindMode` means the controls are being configured rather
   than played; `bindingIndex` is the one action waiting to hear a key. */
let bindMode = false;
let bindingIndex = null;

/** Choose what the next keypress will be bound to. */
function bindTarget(index) {
  bindingIndex = index;
  showBindings();
  setStatus('press the key you want for this');
}

pads.forEach((pad, degree) => {
  pad.addEventListener('pointerdown', (e) => {
    e.preventDefault(); // stop the browser turning the press into a scroll

    if (bindMode) {
      // In bind mode a tap chooses which pad to reassign instead of playing it.
      bindTarget(degree);
      return;
    }

    // Capture routes pointerup to this pad even if the finger drifts off it
    // mid-hold, which otherwise leaves the note ringing forever.
    pad.setPointerCapture(e.pointerId);
    press(e.pointerId, degree);
  });

  const up = (e) => release(e.pointerId);
  pad.addEventListener('pointerup', up);
  pad.addEventListener('pointercancel', up); // finger stolen by a system gesture
});

/**
 * Give `key` to one action, taking it off whatever had it before.
 *
 * Rebinding REPLACES that action's keys with the single one you just pressed,
 * rather than adding to them. The pads ship with two keys each, so "add" would
 * quietly grow the list with no way to shrink it again; "the key you press is
 * the key for this pad" is the behaviour you can predict without being told.
 */
function bindKey(index, key) {
  bindings = bindings.map((keys, i) =>
    i === index ? [key] : keys.filter((k) => k !== key)
  );
  rebuildKeyMap();
  saveBindings();
}

const BIND_HINT = 'tap a pad or a looper button, then press a key  (esc to finish)';

function setBindMode(on) {
  bindMode = on;
  bindingIndex = null;
  releaseAll(); // nothing should still be ringing while you reconfigure
  rebindBtn.classList.toggle('active', on);
  rebindBtn.textContent = on ? 'done' : 'rebind keys';
  setStatus(on ? BIND_HINT : null);
  showBindings();
}

const rebindBtn = document.getElementById('rebind');
rebindBtn.addEventListener('click', () => {
  setBindMode(!bindMode);
  rebindBtn.blur(); // or the spacebar would keep re-triggering this button
});

// In bind mode, clicking a looper button picks it for rebinding instead of
// pressing it. Capture phase, so this runs before looper.js's own handler.
document.getElementById('controls').addEventListener('click', (e) => {
  if (!bindMode) return;
  const btn = e.target.closest('button[data-action]');
  if (!btn) return;
  e.stopPropagation();
  bindTarget(ACTIONS.indexOf(btn.dataset.action));
}, true);

addEventListener('keydown', (e) => {
  // Leave the browser's own shortcuts alone - ctrl+R must still reload.
  if (e.metaKey || e.ctrlKey || e.altKey) return;

  if (bindMode) {
    if (e.key === 'Escape') return setBindMode(false);
    if (bindingIndex !== null) {
      e.preventDefault();
      bindKey(bindingIndex, normalizeKey(e.key));
      bindingIndex = null;
      showBindings();
      setStatus(BIND_HINT);
    }
    return;
  }

  const key = normalizeKey(e.key);
  const action = keyMap.get(key);
  if (action === undefined) return;

  e.preventDefault(); // keys like space would otherwise scroll or re-click
  if (e.repeat) return; // holding a key repeats it; a held chord is one press

  // Each key holds independently, exactly as each finger does - a pad answering
  // to both 'a' and '1' must not go quiet because you let go of one of them.
  if (typeof action === 'number') press('key:' + key, action);
  else if (action === 'pedal') pedal();
  else if (action === 'playstop') togglePlay();
  else if (action === 'clear') clearLoop();
  else if (action === 'arp') setArp(!arpOn);
});

addEventListener('keyup', (e) => {
  const key = normalizeKey(e.key);
  if (typeof keyMap.get(key) === 'number') release('key:' + key);
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
      setStatus('audio blocked &mdash; tap again, or check the silent switch');
    }
  }, 250);
});

/* ------------------------------------------------------- key/mode pickers -- */

const keySelect = document.getElementById('keysel');
const modeSelect = document.getElementById('modesel');

NOTE_NAMES.forEach((name, pitch) => keySelect.add(new Option(name, pitch)));

// Changing key while a loop is running transposes the loop, free of charge:
// the looper records chord numbers rather than pitches, so "chord 5" just
// means the fifth chord of whichever key is selected now.
keySelect.addEventListener('change', () => {
  setKey(Number(keySelect.value), keyMode);
  keySelect.blur();
});
modeSelect.addEventListener('change', () => {
  setKey(keyPitch, modeSelect.value);
  modeSelect.blur();
});

const startingKey = loadKey();
keySelect.value = String(startingKey.pitch);
modeSelect.value = startingKey.mode === 'minor' ? 'minor' : 'major';
setKey(Number(keySelect.value), modeSelect.value);

showBindings();

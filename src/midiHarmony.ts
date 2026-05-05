import { parseMidi } from "midi-file";

export type MidiCorpusFile = {
  name: string;
  path: string;
  relativePath: string;
  size: number;
};

export type AgentHarmonyVoice = {
  chordDegree: number;
  frequency: number;
  instrument: string;
  midi: number;
  program: number;
};

export type AquariumHarmonyFrame = {
  agentVoices: Record<string, AgentHarmonyVoice>;
  chordLabel: string;
  corpusLabel: string;
  sourceName: string;
  sourcePath: string;
};

type ChordState = {
  key: string;
  label: string;
  pitchClasses: number[];
  salience: number;
};

export type HarmonySource = {
  chords: ChordState[];
  filesSeen: number;
  instruments: Array<{ count: number; name: string; program: number }>;
  markov: Map<string, string[]>;
  sourceName: string;
  sourcePath: string;
};

export type HarmonyRuntime = {
  frame: AquariumHarmonyFrame;
  next: () => AquariumHarmonyFrame;
};

const defaultCorpusLabel = "Classical MIDIRip";
const noteNames = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const gmProgramNames = [
  "Acoustic Grand Piano", "Bright Piano", "Electric Grand Piano", "Honky-tonk Piano",
  "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
  "Celesta", "Glockenspiel", "Music Box", "Vibraphone", "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
  "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ", "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
  "Nylon Guitar", "Steel Guitar", "Jazz Guitar", "Clean Guitar", "Muted Guitar", "Overdrive Guitar", "Distortion Guitar", "Guitar Harmonics",
  "Acoustic Bass", "Finger Bass", "Pick Bass", "Fretless Bass", "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
  "Violin", "Viola", "Cello", "Contrabass", "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
  "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2", "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
  "Trumpet", "Trombone", "Tuba", "Muted Trumpet", "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
  "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax", "Oboe", "English Horn", "Bassoon", "Clarinet",
  "Piccolo", "Flute", "Recorder", "Pan Flute", "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
  "Lead 1 Square", "Lead 2 Saw", "Lead 3 Calliope", "Lead 4 Chiff", "Lead 5 Charang", "Lead 6 Voice", "Lead 7 Fifths", "Lead 8 Bass+Lead",
  "Pad 1 New Age", "Pad 2 Warm", "Pad 3 Polysynth", "Pad 4 Choir", "Pad 5 Bowed", "Pad 6 Metallic", "Pad 7 Halo", "Pad 8 Sweep",
  "FX 1 Rain", "FX 2 Soundtrack", "FX 3 Crystal", "FX 4 Atmosphere", "FX 5 Brightness", "FX 6 Goblins", "FX 7 Echoes", "FX 8 Sci-fi",
  "Sitar", "Banjo", "Shamisen", "Koto", "Kalimba", "Bagpipe", "Fiddle", "Shanai",
  "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock", "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
  "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet", "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];

export async function loadShuffledDefaultHarmony(agentIds: string[]) {
  const listing = await fetchJson<{ files: MidiCorpusFile[]; root: string }>("/midi-corpus/list");
  const files = shuffle(listing.files, Date.now()).filter((file) => file.size > 0);
  for (const file of files.slice(0, 40)) {
    try {
      const source = await loadHarmonySource(file);
      return { files, frame: createHarmonyRuntime(source, agentIds).frame, source };
    } catch {
      // MIDI archives contain plenty of weird little fossils. Keep digging.
    }
  }
  throw new Error("no parseable MIDI harmony source found in the default corpus");
}

export async function loadNextHarmony(files: MidiCorpusFile[], agentIds: string[], currentPath?: string) {
  const candidates = shuffle(files.filter((file) => file.path !== currentPath), Date.now());
  for (const file of candidates.slice(0, 50)) {
    try {
      const source = await loadHarmonySource(file);
      return { frame: createHarmonyRuntime(source, agentIds).frame, source };
    } catch {
      // Keep shuffling; bad files do not get to be the main character.
    }
  }
  throw new Error("no alternate parseable MIDI file found");
}

export async function pickHarmonyFolder(agentIds: string[]) {
  const picker = (window as any).showDirectoryPicker as undefined | (() => Promise<FileSystemDirectoryHandle>);
  if (!picker) {
    throw new Error("this browser does not expose a folder picker; use the Vite default corpus endpoint");
  }
  const root = await picker();
  const files: MidiCorpusFile[] = [];
  await collectHandleMidiFiles(root, files);
  const loaded = await loadFromFileHandles(files, agentIds);
  return { files, ...loaded };
}

export function createHarmonyRuntime(source: HarmonySource, agentIds: string[]): HarmonyRuntime {
  const salient = [...source.chords].sort((a, b) => b.salience - a.salience);
  let state = salient[0] ?? source.chords[0];
  let step = 0;
  const next = () => {
    const options = source.markov.get(state.key) ?? salient.slice(0, 5).map((chord) => chord.key);
    const nextKey = options[(step * 7 + state.key.length) % Math.max(options.length, 1)];
    state = source.chords.find((chord) => chord.key === nextKey) ?? salient[(step + 1) % Math.max(salient.length, 1)] ?? state;
    step += 1;
    return frameFromState(source, state, agentIds, step);
  };
  return { frame: frameFromState(source, state, agentIds, step), next };
}

async function loadFromFileHandles(files: MidiCorpusFile[], agentIds: string[]) {
  for (const file of shuffle(files, Date.now()).slice(0, 40)) {
    const handle = (file as any).handle as FileSystemFileHandle | undefined;
    if (!handle) continue;
    try {
      const source = buildHarmonySource(file, await (await handle.getFile()).arrayBuffer());
      return { frame: createHarmonyRuntime(source, agentIds).frame, source };
    } catch {
      // Same fossil policy, now with permissions.
    }
  }
  throw new Error("selected folder did not contain a parseable MIDI harmony source");
}

async function loadHarmonySource(file: MidiCorpusFile) {
  const response = await fetch(`/midi-corpus/file?path=${encodeURIComponent(file.path)}`, { cache: "no-store" });
  if (!response.ok) throw new Error(`MIDI fetch failed: ${response.status}`);
  return buildHarmonySource(file, await response.arrayBuffer());
}

function buildHarmonySource(file: MidiCorpusFile, buffer: ArrayBuffer): HarmonySource {
  const midi = parseMidi(new Uint8Array(buffer));
  const ticksPerBeat = midi.header.ticksPerBeat ?? midi.header.timeDivision ?? 480;
  const bucketSize = Math.max(1, ticksPerBeat * 2);
  const buckets = new Map<number, { low: number; notes: Map<number, number>; weight: number }>();
  const programs = new Map<number, number>();
  const transitions: string[] = [];

  for (const track of midi.tracks) {
    let tick = 0;
    for (const event of track as any[]) {
      tick += event.deltaTime ?? 0;
      if (event.type === "programChange" && typeof event.programNumber === "number") {
        programs.set(event.programNumber, (programs.get(event.programNumber) ?? 0) + 1);
      }
      if (event.type !== "noteOn" || !event.velocity) continue;
      const bucketIndex = Math.round(tick / bucketSize);
      const bucket = buckets.get(bucketIndex) ?? { low: event.noteNumber, notes: new Map<number, number>(), weight: 0 };
      const pitchClass = event.noteNumber % 12;
      bucket.low = Math.min(bucket.low, event.noteNumber);
      bucket.notes.set(pitchClass, (bucket.notes.get(pitchClass) ?? 0) + event.velocity);
      bucket.weight += event.velocity;
      buckets.set(bucketIndex, bucket);
    }
  }

  const statesByKey = new Map<string, ChordState>();
  const sortedBuckets = [...buckets.entries()].sort((a, b) => a[0] - b[0]);
  for (const [, bucket] of sortedBuckets) {
    const pcs = [...bucket.notes.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([pitchClass]) => pitchClass)
      .sort((a, b) => a - b);
    if (pcs.length < 2) continue;
    const root = bucket.low % 12;
    const key = `${root}:${pcs.join(".")}`;
    const existing = statesByKey.get(key);
    if (existing) {
      existing.salience += bucket.weight;
    } else {
      statesByKey.set(key, { key, label: chordLabel(root, pcs), pitchClasses: pcs, salience: bucket.weight });
    }
    transitions.push(key);
  }

  const markov = new Map<string, string[]>();
  for (let index = 0; index < transitions.length - 1; index += 1) {
    const current = transitions[index];
    const next = transitions[index + 1];
    const options = markov.get(current) ?? [];
    options.push(next);
    markov.set(current, options);
  }

  const chords = [...statesByKey.values()]
    .filter((chord) => chord.salience > 0)
    .sort((a, b) => b.salience - a.salience)
    .slice(0, 96);
  if (chords.length < 3) throw new Error("not enough chord material");

  const instruments = [...programs.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([program, count]) => ({ count, name: gmProgramNames[program] ?? `Program ${program + 1}`, program }))
    .slice(0, 16);

  return {
    chords,
    filesSeen: 1,
    instruments: instruments.length ? instruments : [{ count: 1, name: "Acoustic Grand Piano", program: 0 }],
    markov,
    sourceName: file.relativePath || file.name,
    sourcePath: file.path,
  };
}

function frameFromState(source: HarmonySource, chord: ChordState, agentIds: string[], step: number): AquariumHarmonyFrame {
  const agentVoices: Record<string, AgentHarmonyVoice> = {};
  agentIds.forEach((id, index) => {
    const pitchClass = chord.pitchClasses[index % chord.pitchClasses.length];
    const octave = 3 + ((index + Math.floor(step / 2)) % 3);
    const midi = nearestMidiForPitchClass(12 * octave + 12, pitchClass, index);
    const instrument = source.instruments[index % source.instruments.length];
    agentVoices[id] = {
      chordDegree: index % chord.pitchClasses.length,
      frequency: midiToFrequency(midi),
      instrument: instrument.name,
      midi,
      program: instrument.program,
    };
  });
  return {
    agentVoices,
    chordLabel: chord.label,
    corpusLabel: defaultCorpusLabel,
    sourceName: source.sourceName,
    sourcePath: source.sourcePath,
  };
}

function nearestMidiForPitchClass(anchor: number, pitchClass: number, spread: number) {
  let midi = anchor + ((pitchClass - anchor) % 12 + 12) % 12;
  if (midi - anchor > 6) midi -= 12;
  return clampNumber(midi + Math.floor(spread / 3) * 12, 36, 84);
}

function chordLabel(root: number, pitchClasses: number[]) {
  const intervals = pitchClasses.map((pc) => (pc - root + 12) % 12);
  const quality =
    intervals.includes(4) && intervals.includes(7) ? "" :
    intervals.includes(3) && intervals.includes(7) ? "m" :
    intervals.includes(3) && intervals.includes(6) ? "dim" :
    intervals.includes(4) && intervals.includes(8) ? "aug" :
    intervals.includes(5) ? "sus4" :
    intervals.includes(2) ? "sus2" :
    "";
  const extensions = [
    intervals.includes(10) ? "7" : "",
    intervals.includes(11) ? "maj7" : "",
    intervals.includes(2) && !quality.includes("sus2") ? "9" : "",
  ].filter(Boolean).join(" ");
  return `${noteNames[root]}${quality}${extensions ? ` ${extensions}` : ""}`;
}

function midiToFrequency(midi: number) {
  return 440 * 2 ** ((midi - 69) / 12);
}

function shuffle<T>(items: T[], seed: number) {
  const next = mulberry32(seed >>> 0);
  const copy = [...items];
  for (let index = copy.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(next() * (index + 1));
    [copy[index], copy[swap]] = [copy[swap], copy[index]];
  }
  return copy;
}

function mulberry32(seed: number) {
  let state = seed || 0x9e3779b9;
  return () => {
    state += 0x6d2b79f5;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`request failed ${response.status}: ${url}`);
  return (await response.json()) as T;
}

async function collectHandleMidiFiles(directory: FileSystemDirectoryHandle, files: MidiCorpusFile[], prefix = "") {
  for await (const [name, handle] of (directory as any).entries() as AsyncIterable<[string, FileSystemHandle]>) {
    if (files.length >= 5000) return;
    const relativePath = prefix ? `${prefix}/${name}` : name;
    if (handle.kind === "directory") {
      await collectHandleMidiFiles(handle as FileSystemDirectoryHandle, files, relativePath);
      continue;
    }
    if (!/\.(mid|midi)$/i.test(name)) continue;
    const file = await (handle as FileSystemFileHandle).getFile();
    files.push({
      name,
      path: relativePath,
      relativePath,
      size: file.size,
      ...( { handle } as any ),
    });
  }
}

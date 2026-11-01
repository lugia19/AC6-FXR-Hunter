// Gives every FXR a "hunter" external value: ID = HUNTER_BASE + the FXR's own ID. The fxr_hunter DLL serves it per
// FXR, and the value picks how the effect is drawn:
//   0 (no DLL, or the DLL leaves it alone): stock
//   1: hidden
//   2..7: tinted red, green, blue, yellow, cyan, magenta (multiplied, with a boost, so most hues still show)
// The DLL also uses the reads themselves: an ID being read means that FXR is alive right now.
//
// Where the multiplier goes: a particle's color is the product of its appearance colors and its config's
// ParticleModifier color, so particles get it once, on the ParticleModifier (added if missing). Every other action
// with color properties (lights, lens flares, GPU particles, ...) gets it on each of its colors.
//
// Usage: node inject.mjs <ids file out> <effect dir...>
// Rewrites the FXRs in place and writes the injected FXR IDs (one per line) to the ids file.
import { ActionType, ConstantProperty, ExternalValue1Modifier, FXR, Game, ParticleModifier, SteppedProperty, Keyframe } from '@cccode/fxr'
import fs from 'node:fs'
import path from 'node:path'

export const HUNTER_BASE = 1_000_000_000 // must match dll/src/lib.rs
const MAX_EXT_ID = 2 ** 31 - 1

// Index = served value. Must match the DLL's palette order (dll/src/lib.rs PALETTE).
const FACTORS = [
  [1, 1, 1, 1], // 0 stock
  [0, 0, 0, 0], // 1 hidden
  [4, 0.15, 0.15, 1], // 2 red
  [0.15, 4, 0.15, 1], // 3 green
  [0.2, 0.5, 5, 1], // 4 blue
  [3, 3, 0.1, 1], // 5 yellow
  [0.1, 3, 3, 1], // 6 cyan
  [3, 0.1, 3, 1], // 7 magenta
]
const scalarFactors = FACTORS.map(f => (f[0] === 0 ? 0 : 1)) // scalar color props (rare): only hide

function hunterModifier(id, vector) {
  const values = vector ? FACTORS : scalarFactors
  return new ExternalValue1Modifier(id, new SteppedProperty(false, values.map((v, i) => new Keyframe(i, v))))
}

function addModifier(action, key, id) {
  const meta = action.$data.props[key]
  if (!meta || 'field' in meta) return 0 // fields can't carry modifiers
  let prop = action[key]
  if (prop === undefined || prop === null) return 0
  if (typeof prop === 'number' || Array.isArray(prop)) prop = action[key] = new ConstantProperty(prop)
  if (!prop.modifiers) return 0
  const vector = Array.isArray(prop.value ?? null) || (prop.valueType ?? 0) > 0 || (Array.isArray(prop.keyframes?.[0]?.value))
  prop.modifiers.push(hunterModifier(id, vector))
  return 1
}

function inject(fxr) {
  const id = HUNTER_BASE + fxr.id
  let count = 0
  const handled = new Set()
  for (const config of fxr.root.walkConfigs()) {
    const appearance = config.appearance
    if (!appearance?.$data?.isParticle) continue
    if (!config.particleModifier) config.particleModifier = new ParticleModifier()
    handled.add(appearance)
    handled.add(config.particleModifier)
    count += addModifier(config.particleModifier, 'color', id)
    // Hidden also scales the particles to 0: the color multiplier alone leaves black shapes on particles whose blend
    // mode ignores alpha. Scalar props get scalarFactors (0 for hidden, 1 for everything else).
    for (const key of ['scaleX', 'scaleY', 'scaleZ']) count += addModifier(config.particleModifier, key, id)
  }
  for (const action of fxr.root.walkActions()) {
    if (handled.has(action) || action.type === ActionType.ParticleModifier) continue
    if (!action.$data || !('props' in action.$data)) continue
    for (const [key, meta] of Object.entries(action.$data.props)) {
      if ('color' in meta) count += addModifier(action, key, id)
    }
  }
  return count
}

const [idsOut, ...dirs] = process.argv.slice(2)
const ids = new Set()
let failed = 0, untouched = 0, total = 0
const jobs = []
for (const dir of dirs) {
  for (const name of fs.readdirSync(dir)) {
    if (name.endsWith('.fxr')) jobs.push(path.join(dir, name))
  }
}
async function work(file) {
  try {
    // A path, not a buffer: small buffers from readFileSync are views into a shared pool the library misreads.
    const fxr = await FXR.read(file, Game.ArmoredCore6)
    if (HUNTER_BASE + fxr.id > MAX_EXT_ID) throw new Error(`FXR ID ${fxr.id} too large for the hunter ID scheme`)
    const n = inject(fxr)
    if (n === 0) {
      untouched++
      return
    }
    await fxr.saveAs(file, Game.ArmoredCore6)
    ids.add(fxr.id)
    total += n
  } catch (e) {
    failed++
    console.log(`FAILED ${file}: ${e.message}`)
  }
}
const CONCURRENCY = 16
let next = 0
await Promise.all(Array.from({ length: CONCURRENCY }, async () => {
  while (next < jobs.length) {
    const file = jobs[next++]
    await work(file)
    if (next % 2000 === 0) console.log(`${next}/${jobs.length}`)
  }
}))
fs.writeFileSync(idsOut, [...ids].sort((a, b) => a - b).join('\n') + '\n')
console.log(`${jobs.length} FXR files: ${ids.size} distinct IDs injected (${total} modifiers), ${untouched} with nothing to tint, ${failed} failed`)

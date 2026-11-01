# fxr-hunter

Find the FXR IDs behind the effects on screen in Armored Core VI (1.09.1), without relaunching the game: record which
FXRs show up while you do something, then recolor, hide or solo each one live and repeat the action until you know
which is which. Any FXR can also slow down or freeze the game the next time it appears.

## Use

1. `python build.py --game "<...>/Game" --witchy "<...>/WitchyBND.exe"` (`--only sfxbnd_commoneffects` for a quick
   partial build).
2. Launch `AC6_tools\me3\launch-fxr-hunter.bat` (dev profile: the package plus alt saves), or `package\fxr-hunter.me3`
   (own save, `FxrHunter.sl2`).
3. The UI (`fxr_hunter_ui.exe`, always on top) opens by itself with the game and closes with it.
4. In game:
   - **F9**: start recording. Every FXR that shows up (reads its hunter value) gets listed, sorted by ID, with when
     it first appeared and a dot while it's reading now. **F10** stops recording; the list stays. F9 again starts over.
   - In the UI, per FXR: a **color** (stock, red, green, blue, yellow, cyan, magenta), **hidden**, **Solo** (hides
     every other recorded FXR; **Unhide all** undoes it), and **slow on appear**: the next time
     it appears (starts reading after half a second of silence), the game slows down to the slow speed (see F11).
     **Reset looks** clears colors and hidden.
   - **Add FXR ID** puts any FXR on the list by ID (it stays across recordings). **×** removes a row: an added FXR
     for good, a recorded one until the next recording (for good: the **Ignore** button).
   - **Settings…**: an **ignore list** (`fxr_hunter_ignore.txt` next to the DLL: one FXR ID or `a-b` range per line,
     `#` comments; the **Ignore** button on a row appends to it) and an **ID range**, each switchable. Both filter
     recording and the seen log; FXRs added by ID and looks still work. Saved to `fxr_hunter_settings.json` (with the
     slow speed); full builds keep both files.
   - **Log…**: writes the FXRs seen in each interval (default 100 ms) to
     `Game\debug-logs\fxr_hunter_seen_<date>_<time>.tsv`: ms since the log started, clock time, game speed, count,
     FXR IDs. Empty intervals are skipped. Independent of recording.
   - Looks stay until reset, so: record, set looks, repeat the action, adjust, repeat. Some effects only read their
     value when they emit particles, so a new look shows on newly emitted particles.
   - **F11**: slow on/off any time. "Slow speed" in the UI sets how slow (F11 and slow on appear): e.g. 0.1 = 10%
     speed; 0 = frozen (0.0001 internally).
   - **Several at once**: tick rows (Shift+click: a range, Ctrl+A or the header box: all) and use the bar above the
     list: color, Hide, Show, Solo (the ticked ones stay visible), Copy IDs, Ignore, ×.
   - In game, only **F9** / **F10** / **F11**; everything else is in the UI.

   Recordings, seen-log starts and freezes are logged to `Game\debug-logs\fxr_hunter.log`.

An effect is often several FXRs (parent + children, or several spawned together): each is its own entry.

## How it works

- **Merge**: like the original FXR hunter, every bundle's contents move into `sfxbnd_commoneffects` (always loaded),
  so any FXR exists in any mission; the other bundles ship empty. `--no-merge` keeps them separate.
- **Injection** (`fxr/inject.mjs`, [@cccode/fxr](https://github.com/EvenTorset/fxr)): every FXR in every
  `sfx/*.ffxbnd.dcx` gets an `ExternalValue1Modifier` on external value **1,000,000,000 + its own FXR ID**, with a
  stepped curve: 0 stock, 1 hidden, 2-7 tints (multiplied with a boost, so most hues still show). Particles get it on
  their config's ParticleModifier color (once per particle, as in hex-thrusters), and hidden also scales them to 0
  through the ParticleModifier's scaleX/Y/Z (color 0 alone left black shapes on particles whose blend mode ignores
  alpha); other actions with colors (lights,
  lens flares, GPU particles...) on each color. Without the DLL every value reads 0 = stock.
- **DLL** (`dll/`): hooks the external value read `0x142b56480`. For hunter IDs it records when that FXR last read
  its value (= alive right now) and serves the FXR's mode. Because the ID encodes the FXR ID, the hook needs no
  block-to-effect mapping.
- **Freeze** (`dll/src/freeze.rs`): the flipper's frame delta time (`CSFlipperImp+0x268`, the same value
  [ac6-freecam](https://github.com/vswarte/ac6-freecam)'s time control scales) is stored at `0x14175fb80`; that
  store is patched to multiply by a scale first (0.0001 while frozen).
- **UI** (`ui/`, Rust + egui): `fxr_hunter_ui.exe`, which the DLL starts from its own folder and puts in a job object
  that kills it when the game exits. Talks to the DLL over `127.0.0.1:48690` (line commands, JSON state;
  `dll/src/server.rs`), so it can also be started by hand.

## Limits / open questions

- Controller rumble keeps going while frozen (the game never sends the "stop"). Hooking `XInputSetState` in
  XINPUT9_1_0 and XInput1_4 to zero it only made it toggle on and off; dropped.
- Visuals outlive reads: some effects apparently only read their value when emitting particles (GPU particles,
  probably), so their particles stay on screen after they stop reading, and already-emitted particles may not react
  to a new look. That's why the tool records over time instead of freezing and listing what's live.
- An effect that never changes look isn't an FXR (decal, model texture, UI), or uses an action type whose color
  isn't covered.
- Tints multiply: an effect with no red in it can't turn red. "Hidden" is always unambiguous.
- All instances of an FXR share its look.

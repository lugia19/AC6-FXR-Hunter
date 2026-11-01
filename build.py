"""Builds fxr-hunter from an installed, UXM-unpacked copy of Armored Core VI 1.09.1.

    python build.py --game "<...>/ARMORED CORE VI FIRES OF RUBICON/Game" --witchy "<...>/WitchyBND.exe"

Every effect bundle in sfx/ is unpacked, every FXR in it gets the hunter external value (fxr/inject.mjs), and the
bundles are repacked. Needs node/npm, cargo (stable) and WitchyBND 3.x. Writes package/: the me3 profile, the mod
folder, the DLL with its FXR ID list, and the UI exe (the DLL starts it).

    python build.py --code-only

rebuilds just the DLL and the UI into an existing package/ (no game files needed).
"""
import argparse
import os
import shutil
import subprocess

ROOT = os.path.dirname(os.path.abspath(__file__))
WORK = os.path.join(ROOT, "build")
PACKAGE = os.path.join(ROOT, "package")
IDS_FILE = "fxr_hunter_ids.txt"  # must match dll/src/lib.rs
COMMON = "sfxbnd_commoneffects.ffxbnd.dcx"
USER_FILES = ["fxr_hunter_ignore.txt", "fxr_hunter_settings.json"]  # dll/src/settings.rs


def unpacked(path):
    # WitchyBND's naming: x.ffxbnd.dcx -> x-ffxbnd-dcx-wffxbnd
    return os.path.join(os.path.dirname(path), os.path.basename(path).replace(".", "-") + "-wffxbnd")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--game", help="the game's Game folder (with sfx/)")
    parser.add_argument("--witchy", help="WitchyBND.exe (3.x)")
    parser.add_argument("--code-only", action="store_true", help="only rebuild the DLL and the UI into package/")
    parser.add_argument("--only", nargs="*", help="only these bundles (e.g. sfxbnd_commoneffects), for quick tests")
    parser.add_argument("--no-merge", action="store_true", help="keep FXRs in their own bundles")
    args = parser.parse_args()
    if args.code_only:
        assert os.path.isfile(os.path.join(PACKAGE, IDS_FILE)), "no package/ yet: run a full build first"
        build_code()
        print(f"Updated the DLL and UI in {PACKAGE}")
        return
    if not (args.game and args.witchy):
        parser.error("--game and --witchy are required (except with --code-only)")

    sfx = os.path.join(args.game, "sfx")
    names = sorted(f for f in os.listdir(sfx) if f.endswith(".ffxbnd.dcx"))
    if args.only:
        names = [n for n in names if n.split(".")[0] in args.only]
    assert names, f"no effect bundles found in {sfx}"

    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    bundles = [shutil.copy(os.path.join(sfx, n), WORK) for n in names]
    print(f"Unpacking {len(bundles)} bundles...")
    # WitchyBND exits 0 even when it does nothing, so check the results.
    subprocess.run([args.witchy, "-s", *bundles], check=True, capture_output=True)
    for b in bundles:
        assert os.path.isdir(unpacked(b)), f"WitchyBND didn't unpack {b}"

    # Merge everything into commoneffects, which is always loaded, so every FXR can be spawned and overridden
    # anywhere (the original FXR hunter's initialSetup(); AC6_tools\scripts\build_loadalleffects.py). The other
    # bundles are repacked empty so nothing loads twice. The same file in several bundles: the last one wins.
    common = next((b for b in bundles if os.path.basename(b) == COMMON), None)
    if common and not args.no_merge:
        moved = 0
        for b in bundles:
            if b == common:
                continue
            for sub in os.listdir(unpacked(b)):
                src = os.path.join(unpacked(b), sub)
                if not os.path.isdir(src):
                    continue
                dst = os.path.join(unpacked(common), sub)
                os.makedirs(dst, exist_ok=True)
                for f in os.listdir(src):
                    os.replace(os.path.join(src, f), os.path.join(dst, f))
                    moved += 1
        print(f"Merged {moved} files into {COMMON}")

    fxr_dir = os.path.join(ROOT, "fxr")
    shell = os.name == "nt"
    if not os.path.isdir(os.path.join(fxr_dir, "node_modules")):
        subprocess.run(["npm", "install"], cwd=fxr_dir, check=True, shell=shell)
    effect_dirs = [os.path.join(unpacked(b), "effect") for b in bundles if os.path.isdir(os.path.join(unpacked(b), "effect"))]
    ids_path = os.path.join(WORK, IDS_FILE)
    subprocess.run(["node", "inject.mjs", ids_path, *effect_dirs], cwd=fxr_dir, check=True, shell=shell)

    print("Repacking...")
    before = {b: os.path.getmtime(b) for b in bundles}
    subprocess.run([args.witchy, "-s", *[unpacked(b) for b in bundles]], check=True, capture_output=True)
    for b in bundles:
        assert os.path.getmtime(b) != before[b], f"WitchyBND didn't repack {b}"

    # The player's ignore list and settings live in package/ (next to the DLL): keep them.
    kept = {name: open(os.path.join(PACKAGE, name), "rb").read() for name in USER_FILES
            if os.path.isfile(os.path.join(PACKAGE, name))}
    shutil.rmtree(PACKAGE, ignore_errors=True)
    os.makedirs(os.path.join(PACKAGE, "mod", "sfx"))
    for name, data in kept.items():
        open(os.path.join(PACKAGE, name), "wb").write(data)
    for b in bundles:
        shutil.copy(b, os.path.join(PACKAGE, "mod", "sfx"))
    shutil.copy(ids_path, PACKAGE)
    shutil.copy(os.path.join(ROOT, "fxr-hunter.me3"), PACKAGE)
    build_code()
    print(f"Built {PACKAGE}")


def build_code():
    """Builds the DLL and the UI and copies them into package/."""
    for crate, binary in [("dll", "fxr_hunter.dll"), ("ui", "fxr_hunter_ui.exe")]:
        subprocess.run(["cargo", "+stable", "build", "--release"], cwd=os.path.join(ROOT, crate), check=True)
        shutil.copy(os.path.join(ROOT, crate, "target", "release", binary), PACKAGE)


if __name__ == "__main__":
    main()

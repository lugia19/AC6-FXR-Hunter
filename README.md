# FXR Hunter (For Armored Core 6)

The core idea behind this tool is to help you identify the FXR id of an effect through repeated testing, by narrowing it down to 1/5th of the possible IDs each run (takes around 4/5 runs).

This is based off of CCCode's excellent [FXR library](https://github.com/EvenTorset/fxr).

## Prerequisites:

- Unpacked files with UXM
- ModEngine2 and WitchyBND
- Node environment with CCCode's library installed
- Specifying the paths to WitchyBND, the BND you want to search in and the "start ac6" batch file from modengine2 in the js script.

## What does it do?

### Setup:

1) Copies the BND of your choosing into the modengine2 mod folder you specify, and unpack it using WitchyBND
2) Generates "off" versions of all effects in that BND (this helps avoid confusion, so any effect that isn't currently being evaluated will be disabled)
3) (Optional) Performs a sanity check, keeping all the effects disabled and asking you if the effect has actually been disabled (just in case it's not in the BND you selected, for example)
4) (Optional) Allows you to specify a more narrow range to start from (say, one acquired from a previous run of the tool)

### Main loop:
1) Break up the FXRs currently being processed into categories, each one recolored with a different color
2) Repackage the BND with those modified FXRs
3) Start the game
4) Ask you which color the effect has
5) Narrow it down to that subset, close the game, and begin the loop again

### Final touchup (happens once you have less IDs than colors):
1) Assigns each FXR a different color
2) Starts the game to let you see exactly which one is the correct one
3) Closes the game

import {FXR, Game, Node} from '@cccode/fxr';
import path from 'path';
import readline from 'readline';
import {execSync} from 'child_process';
import toml from '@iarna/toml'
import fs from 'fs-extra'

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
});


//These are the parameters you need to change yourself:
const ac6_sfx_directory = "D:\\SteamLibrary\\steamapps\\common\\ARMORED CORE VI FIRES OF RUBICON\\Game\\sfx"
const modengine2_directory = "C:\\Users\\lugia19\\Desktop\\Programs\\AC6_tools\\ModEngine-2.1.0.0-win64"
const witchybnd_path = "C:\\Users\\lugia19\\Desktop\\Programs\\AC6_tools\\WitchyBND\\WitchyBND.exe"


//These are the colors used to identify effects in the color hunter mode.
const colors = [
    {
        rgba: [1, 0, 0], // Red
        name: 'Red',
        code: '\x1b[91m'
    },
    {
        rgba: [0, 1, 0], // Green
        name: 'Green',
        code: '\x1b[92m'
    },
    {
        rgba: [0, 0, 1], // Blue
        name: 'Blue',
        code: '\x1b[96m' // Note: This is actually Cyan for better readability.
    },
    {
        rgba: [1, 1, 0], // Yellow
        name: 'Yellow',
        code: '\x1b[93m'
    },
    {
        rgba: [1, 1, 1], // White
        name: 'White',
        code: '\x1b[37m'
    }
];
colors.forEach(color => color.rgba.push(1))

//region Common helper methods
FXR.prototype.clone = function() {
    return FXR.fromJSON(this.toJSON())
}

Node.prototype.clone = function() {
    return Node.fromJSON(this.toJSON())
}

function getFilenameFromFXRID(fxrID) {
    if (typeof fxrID === 'number') fxrID = fxrID.toString();
    if (!fxrID.startsWith('f')) fxrID = 'f' + fxrID.padStart(9, '0');  // Pad to ensure the ID part has 9 digits after 'f'
    if (!fxrID.endsWith('.fxr')) fxrID += '.fxr';
    return fxrID;
}

function getFXRIDFromFilename(filename) {
    if (filename.endsWith('.fxr')) filename = filename.slice(0, -4);
    if (filename.startsWith('f')) filename = filename.slice(1);
    return parseInt(filename, 10);
}

/**
 * Given an FXR, returns a list containing all the referenced FXRs (this list includes itself)
 * @param fxr
 * @returns {*[]}
 */
function findProxiedFXRIDs(fxr) {
    let root = fxr.root
    let stack = [root];
    let result = [];

    while (stack.length > 0) {
        let node = stack.pop();
        if (node.type === 2001) {
            result.push(node.sfx);
        }
        stack.push(...node.getNodes(Game.ArmoredCore6));
    }

    return result;
}

const prompt = (query) => new Promise((resolve) => rl.question(query, resolve));

/**
 * Takes a hex code and spits out an rgba array.
 * @param hex
 * @returns {(number|number)[]}
 */
function hexToRgba(hex) {
    let r = parseInt(hex.slice(1, 3), 16),
            g = parseInt(hex.slice(3, 5), 16),
            b = parseInt(hex.slice(5, 7), 16),
            a = hex.length === 9 ? parseInt(hex.slice(7, 9), 16) / 255 : 1;
    return [r / 255, g / 255, b / 255, a];
}

/**
 * Runs a command (used for killing/restarting the game, and repacking)
 * @param commandPath
 * @param args
 */
function runCommand(commandPath, args = []) {
    try {
        const options = {
            cwd: path.dirname(commandPath),
            stdio: 'inherit',
        };
        const commandLine = `"${commandPath}" ${args.join(' ')}`;
        execSync(commandLine, options);
        console.log(`${path.basename(commandPath)} executed successfully.`);
    } catch (error) {
        console.error(`Error running ${path.basename(commandPath)}: ${error}`);
        if (error.stdout) {
            console.log(`stdout: ${error.stdout.toString()}`);
        }
        if (error.stderr) {
            console.log(`stderr: ${error.stderr.toString()}`);
        }
    }
}

/**
 * Recolors the given node, takes in a 3/4 number array for targetColor (or a hex code).
 * @param node
 * @param targetColor
 * @param overrideScale
 */
function recolorNode(node, targetColor, overrideScale= 1) {
    node.recolor((currentColor) => {
        let [r, g, b, a = 1] = currentColor;
        if (typeof targetColor === 'string') {
            targetColor = hexToRgba(targetColor);
        }

        let scale = Math.max(r, g, b, 1);
        if (overrideScale !== 1)
            scale = overrideScale

        r /= scale;
        g /= scale;
        b /= scale;

        // Calculate HSV value and saturation
        const min = Math.min(r, g, b);
        const max = Math.max(r, g, b);
        let s = max > 0 ? (max - min) / max : 0;

        // Linear interpolation between the HSV "value" (max) and the target color
        // based on the saturation of the original color.
        r = max * (1 - s) + targetColor[0] * s;
        g = max * (1 - s) + targetColor[1] * s;
        b = max * (1 - s) + targetColor[2] * s;

        if (targetColor.length > 3) {
            a = targetColor[3];
        }

        r *= scale;
        g *= scale;
        b *= scale;

        return [r, g, b, a];
    });
}

/**
 * Kills the game process.
 */
function killGame() {
    const processName = "armoredcore6.exe"
    try {
        const output = execSync(`taskkill /IM "${processName}" /F`);
        console.log(`Output: ${output}`);
    } catch (error) {
        if (error.message.includes("The process \"armoredcore6.exe\" not found.")){
            console.log("Game is not currently running, continuing...")
        } else {
            console.error(`Error: ${error}`);
            if (error.stdout) {
                console.log(`stdout: ${error.stdout.toString()}`);
            }
            if (error.stderr) {
                console.log(`stderr: ${error.stderr.toString()}`);
            }
        }

    }
}

//endregion


//region Helper methods specific to this script.

function forceNodeColor(node, targetColor) {
    node.recolor((currentColor) => {
        if (typeof targetColor === 'string') {
            targetColor = hexToRgba(targetColor);
        }
        return targetColor;
    });
}


async function runWitchy(bnd_dir_or_file) {
    await runCommand(witchybnd_path, ["-p",`"${bnd_dir_or_file}"`]);
}

/**
 * Kills the game if it's running, repacks the given bnd, and restarts the game.
 * @returns {Promise<void>}
 */
async function restartGameAndRepack() {
    killGame()
    await runWitchy(root_bnd_dir);
    await runCommand(modengine2_exe, modengine2_args);
}

/**
 * Resets all effects to be disabled, caching them so that they're not re-saved every time.
 * @returns {Promise<void>}
 */
async function reset_all_effects_to_off() {
    try {
        for (const [fxr_id, fxr] of disabled_fxrs.entries()) {
            const destinationPath = path.join(effects_dir, getFilenameFromFXRID(fxr_id));
            const disabledPath = `${destinationPath}-disabled`;
            if (!(await fs.pathExists(disabledPath))) await fxr.saveAs(disabledPath, Game.ArmoredCore6);

            await fs.copy(disabledPath, destinationPath);
        }
        console.log("Reset all effects back to off.");
    } catch (error) {
        console.error(`Error in reset_effects: ${error}`);
    }
}

/**
 * Given an fxr, it enables all the FXRs proxied by it, and returns a list of their IDs.
 * @param fxr
 * @param collectedIDs
 * @returns {Promise<*[]>}
 */
async function enableAllProxiedFXRs(fxr, collectedIDs = []) {
    collectedIDs.push(fxr.id);  // Add the current FXR ID to the list
    await recolorAndSaveFXR(fxr, effects_dir, [1, 1, 1, 1], 'White');
    // Find any proxied FXR IDs from the current FXR
    let proxied_fxrIDs = findProxiedFXRIDs(fxr);

    for (let proxiedID of proxied_fxrIDs) {
        // Recursively process each proxied FXR and merge their results
        await enableAllProxiedFXRs(original_fxrs.get(proxiedID), collectedIDs);
    }

    return collectedIDs;  // Return the complete list of processed FXR IDs.
    // REMEMBER: THIS INCLUDES THE ORIGINAL FXR ITSELF!
}


async function recolorAndSaveFXR(fxr, destinationDirectory, color, colorName) {
    let fxr_clone;
    try {
        fxr_clone = fxr.clone()
    } catch(error) {
        console.error(error)
    }
    const destinationFilePath = path.join(destinationDirectory, getFilenameFromFXRID(fxr.id));
    forceNodeColor(fxr_clone.root, color)
    //console.log(`Set ${destinationFilePath} to ${colorName} (${color})`)
    await fxr_clone.saveAs(destinationFilePath, Game.ArmoredCore6);
}
//endregion


const modengine2_exe = path.join(modengine2_directory, "modengine2_launcher.exe")
const modengine2_args = ["-t","ac6","-c",".\\config_ac6_fxrhunter.toml"]
const fxrhunter_config_file_path = path.join(modengine2_directory, "config_ac6_fxrhunter.toml")

//This is the base color all effects will have. [0, 0, 0] means invisible.
const off_color = [0, 0, 0, 0]
const modengine2_mod_directory = path.join(modengine2_directory, "fxrhunter")


//So many paths...
const target_ffxbnd = path.join(ac6_sfx_directory, "sfxbnd_commoneffects.ffxbnd.dcx")
const sfx_directory = path.join(modengine2_mod_directory, 'sfx');
const new_bnd_path = path.join(sfx_directory, path.basename(target_ffxbnd));

//Let's get the root_bnd_dir.
const root_bnd_dir = path.join(path.dirname(new_bnd_path), path.basename(new_bnd_path).replace(/\./g, '-') + '-wffxbnd');

//Now the effects_dir...
const effects_dir = path.join(root_bnd_dir, "effect")

const disabled_fxrs = new Map()
const original_fxrs = new Map()

async function cleanup(){
    await fs.remove(fxrhunter_config_file_path)
    try {
        await fs.remove(modengine2_mod_directory);
    } catch (error) {}
}

async function createModEngine2Config(){
    //Copy the existing config data, and modify it.
    let configData = toml.parse(await fs.readFile(path.join(modengine2_directory, "config_armoredcore6.toml"), 'utf-8'));
    let modlist = configData.extension.mod_loader.mods;

    //Disable all other mods.
    for (let inner_mod of modlist){
        inner_mod.enabled = false
    }

    let mod = modlist.find(mod => mod.name === "fxrhunter")
    if (mod) mod.enabled = true
    else
    {
        mod = { enabled: true, name: "fxrhunter", path: "fxrhunter" }
        modlist.push(mod)
    }

    const newTomlContent = toml.stringify(configData);
    await fs.writeFile(fxrhunter_config_file_path, newTomlContent);
}

//Basically, this function handles cleanup, creating the config, and unpacking the BND.
//It pretty much just prepares all the files in the correct places.
async function initialSetup() {
    await cleanup()
    await fs.mkdirp(modengine2_mod_directory)
    await createModEngine2Config()

    //Now that we have the config file set up, we copy the target BND and unpack it.
    try {
        await fs.mkdirp(sfx_directory);
        await fs.copy(target_ffxbnd, new_bnd_path);
        console.log(`BND copied successfully to ${new_bnd_path}`);
    } catch (error) {
        console.error(`Failed to copy file: ${error}`);
    }




    //Let's unpack the BND.
    await runWitchy(new_bnd_path)

    //Let's ensure we're not using DCX_KRAK_MAX...
    const xmlFilePath = path.join(root_bnd_dir, '_witchy-ffxbnd.xml');
    try {
        let fileContent = await fs.readFile(xmlFilePath, 'utf8');
        fileContent = fileContent.replace("DCX_KRAK_MAX", "DCX_DFLT_11000_44_9_15");
        await fs.writeFile(xmlFilePath, fileContent, 'utf8');
    } catch (error) {
        console.error(`Failed to modify XML file: ${error}`);
    }

    console.log("Merging all BNDs into commoneffects...")
    const original_sfx_directory = path.dirname(target_ffxbnd)
    //TL;DR of what I do here.

    //Loop over every .ffxbnd.dcx file in original_sfx_directory and:
    // 1) copy it to sfx_directory
    // 2) unpack it
    // 2.5) Get the newly created folder path
    // 3) Move all the files from every subdirectory into commoneffects
    // 4) Repack the bnd file by doing await runWitchy(root_bnd_dir);

    let ffxbnd_files = (await fs.readdir(original_sfx_directory)).filter(f => f.endsWith('.ffxbnd.dcx'));
    ffxbnd_files = ffxbnd_files.filter(item => item !== path.basename(target_ffxbnd))

    for (let filename of ffxbnd_files) {
        const destination_for_working_bnd = path.join(sfx_directory, path.basename(filename));
        await fs.copy(path.join(original_sfx_directory, filename), destination_for_working_bnd);
        await runWitchy(destination_for_working_bnd) //Unpack
        const working_root_bnd_dir = path.join(path.dirname(destination_for_working_bnd), path.basename(destination_for_working_bnd).replace(/\./g, '-') + '-wffxbnd');

        //We have it unpacked, so merge all the dirs.
        let subdirs = (await fs.readdir(working_root_bnd_dir, { withFileTypes: true }))
        subdirs = subdirs.filter(dirent => dirent.isDirectory())

        for (let dir of subdirs) {
            const working_subdirectory = path.join(working_root_bnd_dir, dir.name);
            const destination_subdirectory = path.join(root_bnd_dir, dir.name)

            for (let file of (await fs.readdir(working_subdirectory))) {
                await fs.move(path.join(working_subdirectory, file), path.join(destination_subdirectory, file), { overwrite: true });
            }
        }

        // Repack the bnd file
        await runWitchy(working_root_bnd_dir);

        // Optionally delete the unpacked folder if no longer needed
        // await fs.remove(root_bnd_dir);

    }

    await runWitchy(root_bnd_dir);
    console.log("Merged all BNDs!")
    console.log("\n".repeat(10))

    //Lets create disabled_fxrs and original_fxrs
    const all_fxr_files = fs.readdirSync(effects_dir).filter(file => file.endsWith('.fxr'));
    for (const file of all_fxr_files) {
        const filePath = path.join(effects_dir, file);
        const fxr = await FXR.read(filePath, Game.ArmoredCore6);
        original_fxrs.set(fxr.id, fxr)
        const disabled_fxr = fxr.clone()
        forceNodeColor(disabled_fxr.root, off_color)
        disabled_fxrs.set(disabled_fxr.id, disabled_fxr)
    }
}

/**
 * This function returns the initial file range.
 * @returns {Promise<*>}
 */
async function get_initial_file_range(){
    //Get all the files, and optionally restrict them to a range.
    let effectFiles = (await fs.readdir(effects_dir)).filter(file => file.endsWith('.fxr'));
    effectFiles.sort();

    // Ask user if they want to restrict the search to a specific ID range
    const useRange = await prompt("Do you want to restrict the search to a specific ID range? (y/n): ");
    if (useRange.toLowerCase().trim() === 'y') {
        let startID = await prompt("Enter the start ID or filename (e.g., f000044100.fxr or 44100): ");
        let endID = await prompt("Enter the end ID or filename (e.g., f000048000.fxr or 48000): ");

        startID = getFilenameFromFXRID(startID);
        endID = getFilenameFromFXRID(endID);

        // Filter effectFiles to only include files within the specified range
        effectFiles = effectFiles.filter(file => file >= startID && file <= endID);

    } else {
        const skipSanityCheck = await prompt("Do you want to perform the sanity check? (y/n): ");
        if (skipSanityCheck.toLowerCase().trim() !== 'n') {
            await restartGameAndRepack();

            let response = await prompt('Is the effect still on? (y/n) ');

            if (response.toLowerCase().trim() === 'y') {
                console.log(`The effect is... probably not an effect, since we've disabled every effect in the game. Or something else weird is going on.`)
                rl.close();
                process.exit();
            }

            console.log(`Continuing to identification.`);
        }
    }
    return effectFiles
}


async function main() {
    await initialSetup();
    await reset_all_effects_to_off();
    let effectFiles = await get_initial_file_range();

    console.log("Which mode would you like to use:");
    console.log("1) Color-based ID finder");
    console.log("2) Single ID tester");
    const runningMode = await prompt("Please select one (1/2): ");

    if (runningMode.toLowerCase().trim() === '1') {
        await colorIDFinder(effectFiles);
    } else {
        await singleIDTester(effectFiles);
    }
    rl.close();
    await cleanup();
}

/**
 * Splits the given array into the specified number of segments.
 * @param arrayToSplit
 * @param numSegments
 * @returns {[]}
 */
function splitArray(arrayToSplit, numSegments) {
    const segmentSize = Math.floor(arrayToSplit.length / numSegments);
    const segments = [];

    for (let i = 0; i < numSegments; i++) {
        const start = i * segmentSize;
        const end = (i + 1 === numSegments) ? arrayToSplit.length : start + segmentSize;
        segments.push(arrayToSplit.slice(start, end));
    }

    return segments;
}
async function promptForColorChoice(segments) {
    console.log("Please identify the color of the effect by entering the number next to the correct color:");
    segments.forEach((seg, index) => {
        const color = colors[index]
        console.log(`${color.code || '\x1b[0m'}` + `${index + 1}. ${color.name} (${seg[0]} to ${seg[seg.length - 1]}) (${seg.length} effect IDs)` + "\x1b[0m");
    });

    let selectedIndex = -1;
    while (selectedIndex < 0 || selectedIndex >= segments.length) {
        const input = await prompt(`Enter your choice (1-${segments.length}) (or r to restart the game, or e to exit early): `);
        if (input.toLowerCase().trim() === 'r') {
            console.log("Recompiling BND and restarting game...");
            await restartGameAndRepack();
            continue;
        } else if (input.toLowerCase().trim() === 'e') {
            killGame();
            await cleanup();
            process.exit();
        }
        selectedIndex = parseInt(input) - 1;
        if (isNaN(selectedIndex) || selectedIndex < 0 || selectedIndex >= segments.length) {
            selectedIndex = -1;
        }
    }

    return selectedIndex;
}

async function colorIDFinder(effectFiles) {
    console.log("Starting color-based ID finder...");

    while (effectFiles.length > colors.length) {
        await reset_all_effects_to_off();

        const segments = splitArray(effectFiles, colors.length);
        for (let color of colors) {
            for (const file of segments[colors.indexOf(color)]) {
                await recolorAndSaveFXR(original_fxrs.get(getFXRIDFromFilename(file)), effects_dir, color.rgba, color.name);
            }
        }
        await restartGameAndRepack();

        const selectedIndex = await promptForColorChoice(segments);
        effectFiles = segments[selectedIndex];
    }

    if (effectFiles.length <= colors.length && effectFiles.length > 0) {
        console.log("Assigning a unique color to each remaining effect file...");
        await reset_all_effects_to_off();
        const color_assignment_messages = [];

        for (let i = 0; i < effectFiles.length; i++) {
            const file = effectFiles[i];
            const color = colors[i % colors.length];
            await recolorAndSaveFXR(original_fxrs.get(getFXRIDFromFilename(file)), effects_dir, color.rgba, color.name);

            color_assignment_messages.push(`${color.code}` + `${file}: Set to ${color.name} (${color.rgba.join(', ')})` + "\x1b[0m");
        }

        console.log("Recompiling BND and restarting game with new settings...");
        await restartGameAndRepack();

        for (const message of color_assignment_messages) {
            console.log(message);
        }

        await prompt("\nPress enter to exit...\n");
        killGame();
    }
}
function createTSVRows(effectsData) {
    const headers = ["ID", "BND", "RESOURCES", "ORIGIN", "COLOR", "PARTICLE BEHAVIOUR", "USEFUL INFO", "REFERENCED FXRS"]
    const tsvRows = [headers.join('\t')];
    for (const id of Object.keys(effectsData)) {
        const row = [id, ...effectsData[id]];
        tsvRows.push(row.join('\t'));
    }
    return tsvRows;
}

async function singleIDTester(effectFiles) {
    console.log("Starting single ID tester...");
    let bndName = await prompt("Please enter the BND name (defaults to commoneffects): ");
    if (!bndName || bndName === "") bndName = "commoneffects";

    const origin = await prompt("Please enter the Origin (for example, the weapon): ");

    const effectsData = {};

    for (const currentFile of effectFiles) {
        await reset_all_effects_to_off();

        const current_fxr = original_fxrs.get(getFXRIDFromFilename(currentFile));
        console.log("\n".repeat(5));
        console.log(`Currently testing FXR ID ${current_fxr.id}`);

        const isProbablyInvisible = current_fxr.root.nodes.length === 0;
        let proxied_fxrIDs = [];

        if (isProbablyInvisible) {
            console.log("\x1b[91mEffect is probably invisible! Skipping...\x1b[0m");
        } else {
            await recolorAndSaveFXR(current_fxr, effects_dir, [1, 1, 1, 1], "White");
            proxied_fxrIDs = await enableAllProxiedFXRs(current_fxr);
            proxied_fxrIDs = proxied_fxrIDs.filter(id => id !== current_fxr.id);

            killGame();
            await runWitchy(root_bnd_dir)
            await runCommand(modengine2_exe, modengine2_args);

            if (proxied_fxrIDs.length > 0) {
                console.log(`This FXR also proxies the following IDs (which have also been left enabled): ${proxied_fxrIDs}`);
                for (const proxied_fxrID of proxied_fxrIDs) {
                    if (proxied_fxrID in effectsData) {
                        console.log(`${proxied_fxrID} was previously logged as ${effectsData[proxied_fxrID][5]}`);
                    }
                }
            }
        }

        let effectDescription = "";
        if (isProbablyInvisible) {effectDescription = "Skipped as it has no nodes/containers, likely not visible.";}
        else {
            effectDescription = await prompt(`\nEnter description for this effect (Defaults to "Unknown, not visible."): `);
            if (!effectDescription || effectDescription === "") effectDescription = "Unknown, not visible.";
        }
        effectsData[current_fxr.id] = [bndName.replaceAll("\t", "    "), "", origin.replaceAll("\t", "    "), "", "", effectDescription.replaceAll("\t", "    "), proxied_fxrIDs.join(",")];
    }

    const tsvRows = createTSVRows(effectsData);
    const tsv_path = path.join(modengine2_directory, `${bndName}-${origin}.tsv`);
    await fs.writeFile(tsv_path, tsvRows.join('\n'));
    console.log(`TSV data written to ${tsv_path}`);
}


main().then(r => console.log("Done."));
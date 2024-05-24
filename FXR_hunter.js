import { FXR, Game } from '@cccode/fxr';
import fs from 'fs/promises';
import fssync from 'fs';
import path from 'path';
import readline from 'readline';
import { execSync } from 'child_process';
import toml from '@iarna/toml'

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
});


//These are the parameters you need to change yourself:

//The effects_bnd where you want to try and find the effect (usually commoneffects)
const effects_bnd = "D:\\SteamLibrary\\steamapps\\common\\ARMORED CORE VI FIRES OF RUBICON\\Game\\sfx\\sfxbnd_commoneffects.ffxbnd.dcx";

//The directory of an active mod (ideally empty)
const modengine2_directory = "C:\\Users\\lugia19\\Desktop\\Programs\\AC6 tools\\ModEngine-2.1.0.0-win64";

const witchybnd_path = "C:\\Users\\lugia19\\Desktop\\Programs\\AC6 tools\\WitchyBND\\WitchyBND.exe";
const run_ac6_bat = "C:\\Users\\lugia19\\Desktop\\Programs\\AC6 tools\\ModEngine-2.1.0.0-win64\\launchmod_armoredcore6.bat";








//This is the base color all effects will have. [0, 0, 0] means invisible.
const off_color = [0, 0, 0, 0]
const mod_name = "fxrhunter"
const modengine2_mod_directory = path.join(modengine2_directory, mod_name)
//Let's clean up after ourselves...
try {
    await fs.access(modengine2_mod_directory);
    fssync.rmSync(modengine2_mod_directory, { recursive: true, force: true });
} catch (error) {}

await fs.mkdir(modengine2_mod_directory, { recursive: true })
const ac6_config_file = path.join(modengine2_directory, "config_armoredcore6.toml")
const ac6_config_backup = path.join(modengine2_directory, "config_armoredcore6-backup.toml")

//Let's back up the config file as to preserve comments...
try {
    await fs.access(ac6_config_backup);
    //If it already exists, we don't do anything.
} catch {
    // If the backup does not exist, create it
    await fs.copyFile(ac6_config_file, ac6_config_backup);
}

killGame();
await toggleFXRhunter(true)

//Let's copy and extract the BND into the mod folder...
const sfx_directory = path.join(modengine2_mod_directory, 'sfx');
await fs.mkdir(sfx_directory, { recursive: true });

const new_bnd_path = path.join(sfx_directory, path.basename(effects_bnd));
try {
    await fs.copyFile(effects_bnd, new_bnd_path);
    console.log(`BND copied successfully to ${new_bnd_path}`);
} catch (error) {
    console.error(`Failed to copy file: ${error}`);
}

//Let's get the root_bnd_dir, and clean it up if it already exists.
const root_bnd_dir = path.join(path.dirname(new_bnd_path), path.basename(new_bnd_path).replace(/\./g, '-') + '-wffxbnd');

// Check if root_bnd_dir exists and delete it if it does
try {
    await fs.rm(root_bnd_dir, { recursive: true, force: true });
    console.log(`Existing directory root_bnd_dir removed`);
} catch (error) {
    console.error(`Failed to remove existing directory ${root_bnd_dir}: ${error}`);
}

//Now the effects_dir...
const effects_dir = path.join(root_bnd_dir, "effect")

await runCommand(witchybnd_path, [new_bnd_path])

//Let's ensure we're not using DCX_KRAK_MAX...
const xmlFilePath = path.join(root_bnd_dir, '_witchy-ffxbnd.xml');
try {
    let fileContent = await fs.readFile(xmlFilePath, 'utf8');
    fileContent = fileContent.replace("DCX_KRAK_MAX", "DCX_DFLT_11000_44_9_15");
    await fs.writeFile(xmlFilePath, fileContent, 'utf8');
} catch (error) {
    console.error(`Failed to modify XML file: ${error}`);
}


//This is where the "disabled" versions of all effects will be stored:
const all_effects_disabled_dir = `${effects_dir}-off`
const effects_backup_dir = `${effects_dir}-backup`

await reset_effects();

let effectFiles = await fs.readdir(effects_dir);
effectFiles.sort();  // Sort the files alphabetically

// Ask user if they want to restrict the search to a specific ID range
const useRange = await prompt("Do you want to restrict the search to a specific ID range? (y/n): ");
if (useRange.toLowerCase().trim() === 'y') {
    let startID = await prompt("Enter the start ID or filename (e.g., f000044100.fxr or 44100): ");
    let endID = await prompt("Enter the end ID or filename (e.g., f000048000.fxr or 48000): ");

    startID = formatFileID(startID);
    endID = formatFileID(endID);

    // Filter effectFiles to only include files within the specified range
    effectFiles = effectFiles.filter(file => file >= startID && file <= endID);

} else {
    const skipSanityCheck = await prompt("Do you want to perform the sanity check? (y/n): ");
    if (skipSanityCheck.toLowerCase().trim() !== 'n') {
        await runCommand(witchybnd_path, [root_bnd_dir]);
        await runCommand(run_ac6_bat);

        let response = await prompt('Is the effect still on? (y/n) ');

        if (response.toLowerCase().trim() === 'y') {
            console.log(`The effect is not governed by ${path.basename(new_bnd_path)}.`);
            rl.close();
            process.exit(); // Exit the function if the effect is not governed by common effects
        }
        killGame();
        console.log('The effect is governed by commoneffects - continuing to identification.');
    }
}

console.log("Which mode would you like to use:")
console.log("1) Color-based ID finder")
console.log("2) Single ID tester")
const runningMode = await prompt("Please select one (1/2): ")
if (runningMode.toLowerCase().trim() === '1') {
    console.log("Running color-based ID finder...")
    // Colors to use
    let colors = [[1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 1, 0], [1, 1, 1]]; // Red, Green, Blue, Yellow, White
    for (let color of colors){
        color.push(1)    //Make every color use this alpha.
    }

    const colorNames = ['Red', 'Green', 'Blue', 'Yellow', 'White'];
    const colorCodes = {
        'Red': '\x1b[91m',
        'Green': '\x1b[92m',
        'Blue': '\x1b[96m',     //Yes, this is actually Cyan. Did it for readability.
        'Yellow': '\x1b[93m',
        'White': '\x1b[37m',
    };
    mainColorLoop:
    while (effectFiles.length > colors.length) { // Ensure there are enough files to split into four groups
        await reset_effects();

        const segmentSize = Math.floor(effectFiles.length / colors.length);
        const segments = [];

        // Create segments and apply color to each
        for (let i = 0; i < colors.length; i++) {
            const start = i * segmentSize;
            const end = (i + 1 === colors.length) ? effectFiles.length : start + segmentSize;
            segments.push(effectFiles.slice(start, end));

            for (const file of segments[i]) {
                await setColorForFile(file, effects_dir, colors[i], colorNames[i]); // Apply color
            }
        }

        // Repack the BND, start the game.
        await runCommand(witchybnd_path, [root_bnd_dir]);
        await runCommand(run_ac6_bat);

        // Construct color prompt with file ID ranges
        console.log("Please identify the color of the effect by entering the number next to the correct color:");
        segments.forEach((seg, index) => {
            const colorCode = colorCodes[colorNames[index]] || '\x1b[0m'; // Default to no color if not found
            console.log(`${colorCode}` + `${index + 1}. ${colorNames[index]} (${seg[0]} to ${seg[seg.length - 1]}) (${seg.length} effect IDs)` + "\x1b[0m");
        });
        let selectedIndex = -1
        while (selectedIndex < 0 || selectedIndex >= segments.length) {
            const input = await prompt(`Enter your choice (1-${segments.length}) (or r to restart the game, or e to exit early): `);
            if (input.toLowerCase().trim() === 'r') {
                console.log("Recompiling BND and restarting game...");
                killGame();
                await runCommand(witchybnd_path, [root_bnd_dir]);
                await runCommand(run_ac6_bat);
                continue;
            } else if (input.toLowerCase().trim() === 'e') {
                killGame();
                effectFiles = []
                break mainColorLoop;
            }
            selectedIndex = parseInt(input) - 1;
            if (isNaN(selectedIndex) || selectedIndex < 0 || selectedIndex >= segments.length) {
                selectedIndex = -1;
            }
        }

        killGame()
        // Narrow down to the correct segment based on the user's response
        effectFiles = segments[selectedIndex];
    }

    // This block runs after exiting the main loop when effectFiles.length <= colors.length
    if (effectFiles.length <= colors.length && effectFiles.length > 0) {
        console.log("Assigning a unique color to each remaining effect file...");
        await reset_effects();
        let color_assignment_messages = []
        // Loop through the remaining effect files and assign each a unique color
        for (let i = 0; i < effectFiles.length; i++) {
            const file = effectFiles[i];
            const color = colors[i % colors.length]; // Ensure we cycle through colors if there are fewer than four files
            const colorName = colorNames[i % colorNames.length];
            const colorCode = colorCodes[colorName] || '\x1b[0m'; // Default to no color if not found


            // Apply the color to the file
            await setColorForFile(file, effects_dir, color, colorName); // Ensure this call is awaited

            // Print the file ID and its assigned color
            color_assignment_messages.push(`${colorCode}`+`${file}: Set to ${colorName} (${color.join(', ')})` + "\x1b[0m")
        }

        // Recompile and restart the game
        console.log("Recompiling BND and restarting game with new settings...");
        killGame();
        await runCommand(witchybnd_path, [root_bnd_dir]);
        await runCommand(run_ac6_bat);

        for (let message of color_assignment_messages)
            console.log(message)

        await prompt("\nPress enter to exit...\n")
        killGame();
    }
} else {
    console.log("Running single ID tester...");
    // Ask for BND name and Origin
    let bndName = await prompt("Please enter the BND name (defaults to commoneffects): ");
    if (!bndName || bndName === "") {
        bndName = "commoneffects"
    }
    const origin = await prompt("Please enter the Origin (for example, the weapon): ");

    const effectsData = {}; // Store the effect details

    for (let i = 0; i < effectFiles.length; i++) {
        const currentFile = effectFiles[i];

        // Reset all files to 'off' first
        await reset_effects();

        // Set the current file to white
        let current_fxr = await setColorForFile(currentFile, effects_dir, [1, 1, 1, 1], 'White');
        let isProbablyInvisible = current_fxr.root.nodes.length === 0
        let proxied_fxrIDs = await enableAllProxiedFXRs(current_fxr.id)
        proxied_fxrIDs = proxied_fxrIDs.filter(id => id !== current_fxr.id);
        // Repack and restart the game
        await runCommand(witchybnd_path, [root_bnd_dir]);
        await runCommand(run_ac6_bat);

        // Prompt for effect description
        console.log("\n".repeat(5))
        console.log(`Currently testing FXR ID ${IDfromFile(currentFile)}`)
        if (isProbablyInvisible)
            console.log("\x1b[91mEffect is probably invisible! Skipping...\x1b[0m")
        if (proxied_fxrIDs.length > 0) {
            console.log(`This FXR also proxies the following IDs (which have also been left enabled): ${proxied_fxrIDs}`)
            for (let proxied_fxrID of proxied_fxrIDs) {
                if (proxied_fxrID in effectsData) {
                    console.log(`${proxied_fxrID} was previously logged as ${effectsData[proxied_fxrID][5]}`)
                }
            }
        }
        let effectDescription = ""
        if (isProbablyInvisible) {
            effectDescription = "Skipped as it has no nodes/containers, likely not visible."
        } else {
            effectDescription = await prompt(`\nEnter description for this effect (Defaults to "Unknown, not visible."): `);
            if (!effectDescription || effectDescription === "") {
                effectDescription = "Unknown, not visible."
            }
        }

        if (proxied_fxrIDs.length > 0) {
            effectDescription += ` - Proxies IDs: ${proxied_fxrIDs}`
        }
        // Optionally kill the game if needed
        killGame();

        // Store the effect description with its ID
        const id = IDfromFile(currentFile);
        effectsData[id] = [bndName, "", origin, "", "", effectDescription];
    }
    //Create and print the data in CSV format...

    const csvRows = [["ID", "BND", "RESOURCES", "ORIGIN", "COLOR", "PARTICLE BEHAVIOUR", "USEFUL INFO"].join(',')]; // Create the header row
    for (const id of Object.keys(effectsData)) {
        const row = [id, ...effectsData[id]];
        csvRows.push(row.join(','));
    }
    console.log(csvRows.join('\n'));
}

//Let's clean up, like reverting the config...
rl.close();
//await toggleFXRhunter(false) - No need to toggle it off if we're copying back the backup anyway.
await fs.copyFile(ac6_config_backup, ac6_config_file);
await fs.rm(ac6_config_backup)

try {
    await fs.access(modengine2_mod_directory);
    fssync.rmSync(modengine2_mod_directory, { recursive: true, force: true });
} catch (error) {}


function IDfromFile(filename) {
    const regex = /f0+(\d+)\.fxr$/;
    const match = filename.match(regex);
    return match ? match[1] : null;
}


function formatFileID(fileID) {
    if (typeof fileID === 'number') {
        fileID = fileID.toString();
    }

    // Check if the ID already starts with 'f', otherwise prepend it
    if (!fileID.startsWith('f')) {
        fileID = 'f' + fileID.padStart(9, '0');  // Pad to ensure the ID part has 9 digits after 'f'
    }
    if (!fileID.endsWith('.fxr')) {
        fileID += '.fxr';  // Append the '.fxr' extension if not present
    }
    return fileID;
}

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

async function toggleFXRhunter(shouldBeOn) {
    let configData = toml.parse(await fs.readFile(ac6_config_file, 'utf-8'));
    let modlist = configData.extension.mod_loader.mods;

    let mod = modlist.find(mod => mod.name === mod_name)
    if (mod) {
        mod.enabled = shouldBeOn
    }
    else
    {
        mod = { enabled: shouldBeOn, name: "fxrhunter", path: "fxrhunter" }
        modlist.push(mod)
    }

    const newTomlContent = toml.stringify(configData);
    await fs.writeFile(ac6_config_file, newTomlContent);
}

function runCommand(commandPath, args = []) {
    try {
        const options = {
            cwd: path.dirname(commandPath), // Set working directory to the command file's directory
            stdio: 'inherit'               // Inherits the standard input, output, and error streams
        };
        // Ensure all arguments are correctly quoted
        const commandLine = `"${commandPath}" ${args.map(arg => `"${arg}"`).join(" ")}`;
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
 * Resets effects by deleting the target directory and copying everything from the source directory to it.
 * @param {string} sourceDir Path of the source directory.
 * @param {string} targetDir Path of the target directory to replace.
 */
async function copy_directory(sourceDir, targetDir) {
    try {
        // Remove the target directory if it exists
        await fs.rm(targetDir, { recursive: true, force: true });

        // Recreate the target directory
        await fs.mkdir(targetDir, { recursive: true });

        // Read all contents from the source directory
        const entries = await fs.readdir(sourceDir, { withFileTypes: true });

        // Copy each entry from source directory to target directory
        for (const entry of entries) {
            const srcPath = path.join(sourceDir, entry.name);
            const destPath = path.join(targetDir, entry.name);

            if (entry.isDirectory()) {
                // If it's a directory, recursively copy it
                await copy_directory(srcPath, destPath);
            } else {
                // If it's a file, copy it
                await fs.copyFile(srcPath, destPath);
            }
        }
    } catch (error) {
        console.error(`An error occurred when copying dirs: ${error.message}`);
    }
}

async function reset_effects() {
    try {
        // Check if all_effects_disabled_dir exists
        const dirExists = await fs.access(all_effects_disabled_dir).then(() => true).catch(() => false);

        if (!dirExists) {
            // Check if backup already exists to avoid overwriting it
            const backupExists = await fs.access(effects_backup_dir).then(() => true).catch(() => false);

            if (!backupExists) {
                // Move `effects` to `effects-backup`
                await fs.rename(effects_dir, effects_backup_dir);
            }

            // Use copy_directory to copy from `effects-backup` to `all_effects_disabled_dir`
            await copy_directory(effects_backup_dir, all_effects_disabled_dir);

            // Disable all FXR files in `all_effects_disabled_dir`
            const files = await fs.readdir(all_effects_disabled_dir);
            for (const file of files) {
                if (file.endsWith('.fxr')) {
                    await setColorForFile(file, all_effects_disabled_dir, off_color, "Off"); // Set color to black to disable
                }
            }
        }

        // Continue with the reset as normal
        await copy_directory(all_effects_disabled_dir, effects_dir);
        console.log("Reset all effects back to off.")
    } catch (error) {
        console.error(`Error in reset_effects: ${error}`);
    }
}
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

async function enableAllProxiedFXRs(fxrID, collectedIDs = []) {
    const fileName = formatFileID(fxrID);  // Ensure ID is in the correct format
    collectedIDs.push(fxrID);  // Add the current FXR ID to the list

    let current_fxr = await setColorForFile(fileName, effects_dir, [1, 1, 1, 1], 'White');

    // Find any proxied FXR IDs from the current FXR
    let proxied_fxrIDs = findProxiedFXRIDs(current_fxr);

    for (let proxiedID of proxied_fxrIDs) {
        // Recursively process each proxied FXR and merge their results
        await enableAllProxiedFXRs(proxiedID, collectedIDs);
    }

    return collectedIDs;  // Return the complete list of processed FXR IDs.
    // REMEMBER: THIS INCLUDES THE ORIGINAL FXR ITSELF!
}

async function setColorForFile(fileName, destinationDirectory, color, colorName) {
    const sourceFilePath = path.join(effects_backup_dir, fileName);
    const destinationFilePath = path.join(destinationDirectory, fileName);
    const fxr = await FXR.read(sourceFilePath, Game.ArmoredCore6);

    // Recolor logic
    fxr.root.recolor(([r, g, b, a]) => {
        const scale = Math.max(r, g, b, 1);
        r /= scale;
        g /= scale;
        b /= scale;

        const min = Math.min(r, g, b);
        const max = Math.max(r, g, b);
        let s = max > 0 ? (max - min) / max : 0;

        r = max * (1 - s) + color[0] * s;
        g = max * (1 - s) + color[1] * s;
        b = max * (1 - s) + color[2] * s;

        if (color.length > 3)
            a = color[3];


        r *= scale;
        g *= scale;
        b *= scale;


        return [r, g, b, a];
    });
    //console.log(`Set ${destinationFilePath} to ${colorName} (${color})`)
    await fxr.saveAs(destinationFilePath, Game.ArmoredCore6);
    return fxr;
}

function prompt(query) {
    return new Promise((resolve) => rl.question(query, resolve));
}


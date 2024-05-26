//This is a helper tool - basically, it takes both the existing FXR ID list (and a new ID list) both in TSV format. I use it to quickly update the master list.
import fs from 'fs';
import csv from 'csv-parser';
import readline from 'readline';

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout
});

function ask(question) {
  return new Promise(resolve => {
    rl.question(question, answer => resolve(answer.replaceAll("\"","")));
  });
}

async function readTSV(filepath) {
  const data = [];
  return new Promise((resolve, reject) => {
    fs.createReadStream(filepath)
      .pipe(csv({ separator: '\t' })) // Set separator to tab for TSV files
      .on('data', (row) => data.push(row))
      .on('end', () => resolve(data))
      .on('error', (error) => reject(error));
  });
}

async function writeTSV(filepath, data) {
  const headers = Object.keys(data[0]);
  const lines = data.map(row => headers.map(header => row[header]).join('\t'));

  // Write header and rows to file
  fs.writeFileSync(filepath, headers.join('\t') + '\n' + lines.join('\n'));
}

async function updateMainTSV() {
  try {
    const mainFilePath = await ask('Enter the path to the main TSV file: ');
    const updateFilePath = await ask('Enter the path to the update TSV file: ');
    const outputFilePath = await ask('Enter the path to the output TSV file: ');

    const mainData = await readTSV(mainFilePath);
    const updateData = await readTSV(updateFilePath);

    const updateMap = new Map(updateData.map(row => [row.ID, row]));

    const updatedData = mainData.map(row => updateMap.has(row.ID) ? { ...row, ...updateMap.get(row.ID) } : row);

    await writeTSV(outputFilePath, updatedData);
    console.log('TSV successfully updated and written.');
  } catch (error) {
    console.error('Failed to process TSV files:', error);
  }
  rl.close();
}

await updateMainTSV();

// create-snapshot.js
import { snapshot } from '@webcontainer/snapshot';
import { writeFile } from 'fs/promises';
import path from 'path';

async function createSnapshot() {
  console.log('Starting snapshot of node_modules...');
  
  // Define the source and destination paths
  const sourcePath = './dcl-deps/node_modules_full';
  const outputPath = '../web/assets/node_modules.snapshot';

  try {
    // 1. Generate the binary snapshot from the source directory
    const snapshotData = await snapshot(sourcePath);

    // 2. Write the binary data to the output file in the 'public' directory
    await writeFile(outputPath, snapshotData);

    console.log(`Snapshot created successfully at: ${outputPath}`);
  } catch (error) {
    console.error('Failed to create snapshot:', error);
  }
}

createSnapshot();

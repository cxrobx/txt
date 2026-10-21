// Verify a Tauri updater signature against the pubkey baked into tauri.conf.json.
// Pure Node: minisign = Ed25519 over either the raw file ("Ed") or its
// BLAKE2b-512 hash ("ED", prehashed — what rsign2/Tauri emits).
import { readFileSync } from 'node:fs';
import { createHash, createPublicKey, verify } from 'node:crypto';

const [, , sigPath, filePath, confPath] = process.argv;

// Tauri base64-encodes the whole minisign .sig file.
const sigFile = Buffer.from(readFileSync(sigPath, 'utf8').trim(), 'base64').toString('utf8');
const sigBlob = Buffer.from(sigFile.split('\n')[1].trim(), 'base64');
const sigAlgo = sigBlob.subarray(0, 2).toString('utf8');
const sigKeyId = sigBlob.subarray(2, 10).toString('hex');
const signature = sigBlob.subarray(10, 74);

// pubkey in the config is base64 of the .pub file; its line 2 is the key blob.
const pubField = JSON.parse(readFileSync(confPath, 'utf8')).plugins.updater.pubkey;
const pubFile = Buffer.from(pubField, 'base64').toString('utf8');
const pubBlob = Buffer.from(pubFile.split('\n')[1].trim(), 'base64');
const pubKeyId = pubBlob.subarray(2, 10).toString('hex');
const rawPub = pubBlob.subarray(10, 42);

console.log(`signature algo : ${sigAlgo} (${sigAlgo === 'ED' ? 'prehashed BLAKE2b-512' : 'raw'})`);
console.log(`sig key id     : ${sigKeyId}`);
console.log(`config key id  : ${pubKeyId}`);

if (sigKeyId !== pubKeyId) {
  console.error('\nFAIL: signature was made by a DIFFERENT key than the config trusts.');
  process.exit(1);
}

const content = readFileSync(filePath);
const message = sigAlgo === 'ED' ? createHash('blake2b512').update(content).digest() : content;

// Wrap the raw 32-byte Ed25519 key in SPKI DER so Node will import it.
const spki = Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), rawPub]);
const key = createPublicKey({ key: spki, format: 'der', type: 'spki' });

const ok = verify(null, message, key, signature);
console.log(`\n${ok ? 'PASS' : 'FAIL'}: signature ${ok ? 'VERIFIES' : 'does NOT verify'} against the embedded pubkey`);
process.exit(ok ? 0 : 1);

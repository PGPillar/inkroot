import type { BackupSummary, DeviceInfo, DeviceStatus, Gate } from '../ipc';

export const NOTE_MAX: DeviceInfo = {
  serial: 'NM24B0A1C3',
  model: 'NoteMax',
  profileId: 'note-max',
  displayName: 'Note Max',
  buildId: 'TKQ1.230721.002',
  displayId: 'TKQ1.230721.002',
  incremental: '226',
  firmwareLabel: '4.2',
  slot: 'b',
  battery: 82,
  rooted: false,
  magiskVersion: null,
  bootServiceRunning: false,
};

export const NOTE_MAX_ROOTED: DeviceInfo = {
  ...NOTE_MAX,
  rooted: true,
  magiskVersion: '30.7',
  bootServiceRunning: true,
};

export function adb(info: DeviceInfo, gate: Gate | null = null): DeviceStatus {
  return { kind: 'adb', info, gate };
}

const BOOT = 100663296;
const VBMETA = 65536;
const ROOT = '/Users/you/Documents/Inkroot/NM24B0A1C3';

export const STOCK_BACKUP: BackupSummary = {
  id: 'NM24B0A1C3/2026-10-25_1042',
  serial: 'NM24B0A1C3',
  createdAt: '2026-10-25T10:42:00Z',
  model: 'NoteMax',
  firmware: 'TKQ1.230721.002',
  slot: 'b',
  stock: true,
  verified: true,
  source: 'inkroot',
  path: `${ROOT}/2026-10-25_1042`,
  files: [
    { name: 'boot_a.img', size: BOOT, sha256: '1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09' },
    { name: 'boot_b.img', size: BOOT, sha256: '2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc' },
    { name: 'vbmeta_a.img', size: VBMETA, sha256: '7c1d0e5a93b84f2a6d5e0b1c4a8f3e92d6b7a05c1e48f9d3a2b6c7d8e9f01a2b' },
    { name: 'vbmeta_b.img', size: VBMETA, sha256: '9e3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f6071a' },
  ],
};

/** A backup taken after the tablet was already rooted, so it is not safe to restore from. */
export const ROOTED_BACKUP: BackupSummary = {
  ...STOCK_BACKUP,
  id: 'NM24B0A1C3/2026-10-24_2105',
  createdAt: '2026-10-24T21:05:00Z',
  stock: false,
  path: `${ROOT}/2026-10-24_2105`,
  files: [
    { name: 'boot_a.img', size: BOOT, sha256: '5d8c0f4e2b7a91c3e6f0a8d2b4c6e8f01a3b5d7f9c1e3a5b7d9f0c2e4a6b8d01' },
    { name: 'boot_b.img', size: BOOT, sha256: '8b2e6a0c4d8f1b3e5a7c9d0f2b4e6a8c0d2f4b6e8a1c3d5f7b9e0a2c4d6f8b13' },
    STOCK_BACKUP.files[2],
    STOCK_BACKUP.files[3],
  ],
};

/** A loose boot image the user imported; enough for rescue, not for root. */
export const IMPORTED_RESCUE_ONLY: BackupSummary = {
  id: 'NM24B0A1C3/2026-10-26_0900',
  serial: 'NM24B0A1C3',
  createdAt: '2026-10-26T09:00:00Z',
  model: 'NoteMax',
  firmware: 'TKQ1.230721.002',
  slot: 'b',
  stock: false,
  verified: true,
  source: 'imported',
  path: `${ROOT}/2026-10-26_0900`,
  files: [{ name: 'boot_b.img', size: BOOT, sha256: STOCK_BACKUP.files[1].sha256 }],
};

/** What a fresh Inkroot backup or a stock import produces. */
export const NEW_BACKUP: BackupSummary = {
  ...STOCK_BACKUP,
  id: 'NM24B0A1C3/2026-10-26_0915',
  createdAt: '2026-10-26T09:15:00Z',
  path: `${ROOT}/2026-10-26_0915`,
};

export const IMPORTED_STOCK: BackupSummary = {
  ...IMPORTED_RESCUE_ONLY,
  id: 'NM24B0A1C3/2026-10-26_0930',
  createdAt: '2026-10-26T09:30:00Z',
  stock: true,
  path: `${ROOT}/2026-10-26_0930`,
};

/** The stock backup after root added the patched image. */
export function withPatched(b: BackupSummary): BackupSummary {
  return {
    ...b,
    files: [
      ...b.files.filter((f) => f.name !== 'patched-boot.img'),
      { name: 'patched-boot.img', size: BOOT, sha256: 'c4a1e9d07b3256f8a0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d' },
    ],
  };
}

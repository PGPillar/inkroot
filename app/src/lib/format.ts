import { FORMAT_COPY } from './copy';
import type { AppInfo, BackupSummary, DeviceStatus } from './ipc';

const UNITS = ['KB', 'MB', 'GB'] as const;

/** 1024-based with one decimal: 100663296 → "96.0 MB". Below 1 KB it shows whole bytes. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  let v = n / 1024;
  let u = 0;
  while (v >= 1024 && u < UNITS.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v.toFixed(1)} ${UNITS[u]}`;
}

export function shortSha(sha: string): string {
  return `${sha.slice(0, 4)}…${sha.slice(-3)}`;
}

/** "backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc" for the active-slot image of a backup. */
export function backupLine(b: BackupSummary | null): string {
  if (b === null) return FORMAT_COPY.noBackup;
  const name = `boot_${b.slot}.img`;
  const file = b.files.find((f) => f.name === name);
  const parts = [name, ...(file ? [formatBytes(file.size), `sha ${shortSha(file.sha256)}`] : [])];
  const lead = b.verified && b.stock ? FORMAT_COPY.backupStock : FORMAT_COPY.backupNotOriginal;
  return `${lead} ${parts.join(' · ')}`;
}

export function formatChip(s: DeviceStatus): string {
  switch (s.kind) {
    case 'none':
      return FORMAT_COPY.chipNone;
    case 'unauthorized':
      return FORMAT_COPY.chipUnauthorized;
    case 'edl':
      return FORMAT_COPY.chipEdl;
    case 'adb': {
      const { info } = s;
      if (s.gate === 'unknownModel') return `● ${info.model.toUpperCase()} / ${FORMAT_COPY.chipUnsupported}`;
      const chip = `● ${info.displayName.toUpperCase()} / FW ${info.firmwareLabel} / SLOT_${info.slot.toUpperCase()}`;
      return info.rooted ? `${chip} / ${FORMAT_COPY.chipRoot}` : chip;
    }
  }
}

export function revealLabel(os: AppInfo['os']): string {
  return FORMAT_COPY.reveal[os];
}

/** All backups newest first, grouped by serial (the serial with the newest backup comes first). */
export function groupBySerial(backups: BackupSummary[]): { serial: string; backups: BackupSummary[] }[] {
  const sorted = [...backups].sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  const groups: { serial: string; backups: BackupSummary[] }[] = [];
  for (const b of sorted) {
    const g = groups.find((x) => x.serial === b.serial);
    if (g) g.backups.push(b);
    else groups.push({ serial: b.serial, backups: [b] });
  }
  return groups;
}

/** "2026-10-25 10:42" (UTC, as stored). */
export function backupDate(b: BackupSummary): string {
  return b.createdAt.slice(0, 16).replace('T', ' ');
}

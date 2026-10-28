import type { DeviceStatus, Slot, StepId } from '../ipc';
import type { Beat, Branch } from './engine';
import { NOTE_MAX, adb } from './fixtures';

const BOOT = 100663296;
const VBMETA = 65536;
const PARTITIONS = [
  { name: 'boot_a', size: BOOT, beats: 8 },
  { name: 'boot_b', size: BOOT, beats: 8 },
  { name: 'vbmeta_a', size: VBMETA, beats: 2 },
  { name: 'vbmeta_b', size: VBMETA, beats: 2 },
];

export const step = (id: StepId, stepIndex: number): Beat => ({ state: { step: id, stepIndex } });
const log = (line: string, stream?: 'stdout' | 'stderr' | 'info'): Beat => ({ log: line, stream });

export function checkDevice(idx: number): Beat[] {
  return [
    step('checkDevice', idx),
    log('adb devices -l', 'info'),
    log('NM24B0A1C3          device usb:1-1 product:NoteMax model:NoteMax device:NoteMax transport_id:1'),
    { wait: 500 },
    log('adb shell getprop ro.build.display.id'),
    log('TKQ1.230721.002'),
    log('adb shell getprop ro.boot.slot_suffix'),
    log('_b'),
    { wait: 700 },
  ];
}

export function enterEdl(idx: number): Beat[] {
  return [
    step('enterEdl', idx),
    log('adb reboot edl', 'info'),
    { wait: 900 },
    { device: { kind: 'none' } },
    { wait: 1200 },
    { device: { kind: 'edl' } },
    log('Found Qualcomm HS-USB QDLoader 9008 (05c6:9008)'),
    log('edl --memory=ufs --loader=prog_firehose_ddr.elf printgpt'),
    { wait: 900 },
  ];
}

export function readPartitions(idx: number): Beat[] {
  const beats: Beat[] = [step('readPartitions', idx)];
  for (const p of PARTITIONS) {
    beats.push(log(`edl --memory=ufs r ${p.name} ${p.name}.img`, 'info'));
    for (let n = 1; n <= p.beats; n++) {
      beats.push({ progress: { done: Math.round((p.size * n) / p.beats), total: p.size, label: p.name } }, { wait: 250 });
    }
    beats.push(log(`Read ${p.size} bytes from ${p.name}`));
  }
  return beats;
}

export function verifyBackup(idx: number): Beat[] {
  return [
    step('verifyBackup', idx),
    log('sha256sum boot_a.img boot_b.img vbmeta_a.img vbmeta_b.img', 'info'),
    { wait: 900 },
    log('1aeb66361e7720f7246a1e5e221f75ddab9a0137abdb9a6dbbee92a587c33a09  boot_a.img'),
    log('2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc  boot_b.img'),
    log('Backup verified', 'info'),
  ];
}

export function rebootToSystem(idx: number, device: DeviceStatus = adb(NOTE_MAX)): Beat[] {
  return [
    step('rebootToSystem', idx),
    log('edl reset', 'info'),
    { device: { kind: 'none' } },
    { wait: 1500 },
    { device },
    log('Tablet is back on its normal screen', 'info'),
  ];
}

/** Root group 2. With a verified stock backup it is an instant tick; otherwise it runs the whole backup. */
export function safetyCopy(idx: number, o: { alreadyHave: boolean }): Beat[] {
  if (o.alreadyHave) {
    return [step('verifyBackup', idx), log('Found a verified stock backup, skipping the safety copy', 'info'), { wait: 400 }];
  }
  return [...enterEdl(idx), ...readPartitions(idx), ...verifyBackup(idx), ...rebootToSystem(idx)];
}

export function patch(idx: number): Beat[] {
  return [
    step('patch', idx),
    log('Patching boot_b.img with Magisk 30.7', 'info'),
    { wait: 700 },
    log('- Unpacking boot image'),
    log('- Checking ramdisk status'),
    { wait: 700 },
    log('- Patching ramdisk'),
    log('- Repacking boot image -> patched-boot.img'),
    { wait: 700 },
  ];
}

export function writeBoot(idx: number, slot: Slot, image = 'patched-boot.img'): Beat[] {
  const label = `boot_${slot}`;
  const beats: Beat[] = [
    step('write', idx),
    { state: { writing: true } },
    log(`edl --memory=ufs w ${label} ${image}`, 'info'),
  ];
  for (let n = 1; n <= 12; n++) beats.push({ progress: { done: (BOOT * n) / 12, total: BOOT, label } }, { wait: 350 });
  beats.push({ state: { writing: false } }, log(`Wrote ${BOOT} bytes to ${label}`));
  return beats;
}

export function verifyWrite(idx: number, slot: Slot = 'b'): Beat[] {
  return [
    step('verifyWrite', idx),
    log(`edl --memory=ufs r boot_${slot} readback.img`, 'info'),
    { wait: 1200 },
    log('Read-back matches what was written'),
  ];
}

export function reset(idx: number): Beat[] {
  return [step('reset', idx), log('edl reset', 'info'), { wait: 500 }, { device: { kind: 'none' } }, { wait: 600 }];
}

export function waitBoot(idx: number, device: DeviceStatus): Beat[] {
  return [
    step('waitBoot', idx),
    log('Waiting for the tablet to start up', 'info'),
    { wait: 2000 },
    log('adb wait-for-device'),
    { wait: 1500 },
    { device },
    log('Tablet is back', 'info'),
  ];
}

export function verifyRoot(idx: number, version = '30.7'): Beat[] {
  return [
    step('verifyRoot', idx),
    log('adb shell su -c id'),
    log('uid=0(root) gid=0(root) context=u:r:magisk:s0'),
    log(`adb shell /debug_ramdisk/magisk -c`),
    log(`${version}:MAGISKSU`),
    { wait: 600 },
  ];
}

export function cableCheckPrompt(idx: number): Beat[] {
  const again = () => cableCheckPrompt(idx);
  return [
    step('cableCheck', idx),
    {
      prompt: {
        kind: 'cableCheck',
        options: [
          { id: 'have', label: 'I have one', emphasis: 'primary' },
          { id: 'whereToGet', label: 'Where do I get one?', emphasis: 'secondary' },
          { id: 'continueWithout', label: 'Continue without one', emphasis: 'danger' },
        ],
      },
      branches: {
        have: [log('Rescue cable confirmed by the user', 'info')],
        whereToGet: again,
        continueWithout: [
          {
            prompt: {
              kind: 'cableCheckConfirm',
              options: [
                { id: 'confirmContinue', label: 'Continue without a rescue cable', emphasis: 'danger' },
                { id: 'back', label: 'Go back', emphasis: 'primary' },
              ],
            },
            branches: {
              confirmContinue: [log('Continuing without a rescue cable', 'info')],
              back: again,
            },
          },
        ],
      },
    },
  ];
}

/** The non-blocking tips: `meanwhile` runs while they are up (the backend keeps polling). */
export function edlNotFoundOffer(meanwhile: Beat[], retry: Branch = []): Beat {
  return {
    prompt: {
      kind: 'edlNotFound',
      options: [
        { id: 'retry', label: 'Try again', emphasis: 'primary' },
        { id: 'cancel', label: 'Cancel', emphasis: 'secondary' },
      ],
    },
    meanwhile,
    branches: { retry, cancel: [{ cancelled: true }] },
  };
}

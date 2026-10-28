import type { DeviceStatus, FlowResult, FlowState, Prompt, RescueOutcome, Settings } from '../ipc';
import type { Beat, Scenario } from './engine';
import {
  cableCheckPrompt, checkDevice, edlNotFoundPrompt, enterEdl, patch, readPartitions, rebootToSystem,
  reset, safetyCopy, step, verifyBackup, verifyRoot, verifyWrite, waitBoot, writeBoot,
} from './fragments';
import {
  IMPORTED_RESCUE_ONLY, IMPORTED_STOCK, NEW_BACKUP, NOTE_MAX, NOTE_MAX_ROOTED, ROOTED_BACKUP,
  STOCK_BACKUP, adb, withPatched,
} from './fixtures';

const log = (line: string, stream?: 'stdout' | 'stderr' | 'info'): Beat => ({ log: line, stream });
const PLAIN = adb(NOTE_MAX);
const ROOTED = adb(NOTE_MAX_ROOTED);
const EDL: DeviceStatus = { kind: 'edl' };
const NONE: DeviceStatus = { kind: 'none' };

// ---- flow scripts -------------------------------------------------------------------------

const backupFlow = (): Beat[] => [
  ...checkDevice(1), ...enterEdl(2), ...readPartitions(3), ...verifyBackup(4), ...rebootToSystem(4),
  { done: { backup: NEW_BACKUP } },
];

const importFlow = (result: FlowResult): Beat[] => [
  step('pickImage', 1),
  log('Reading boot_b.stock.img'),
  { wait: 600 },
  step('stockCheck', 2),
  log('sha256sum boot_b.stock.img'),
  { wait: 900 },
  log('Matches the known stock image for TKQ1.230721.002'),
  step('chooseSlot', 3),
  {
    prompt: {
      kind: 'chooseSlot',
      options: [
        { id: 'a', label: 'Slot A', emphasis: 'secondary' },
        { id: 'b', label: 'Slot B', emphasis: 'primary' },
      ],
    },
    branches: {},
  },
  step('saveImport', 3),
  log('Copying into the Inkroot folder'),
  { wait: 700 },
  { done: result },
];

const TABLET_STARTING: Omit<Prompt, 'id'> = {
  kind: 'tabletStarting',
  options: [{ id: 'homeScreen', label: 'Done — I can see my home screen', emphasis: 'primary' }],
};

/** Rescue step 4: written and checked; watch for the tablet, ending the way `outcome` says. */
const rescueWait = (outcome: RescueOutcome): Beat[] => {
  const watch: Beat[] = [
    step('waitBoot', 4),
    log('Written and checked: boot_b on the tablet matches your safety copy', 'info'),
    log('Waiting up to 5 minutes for the tablet to start (Inkroot only sees it if USB debugging is on)', 'info'),
    log('Tablet not visible over USB right now', 'info'),
  ];
  const end = (result: FlowResult, ...beats: Beat[]): Beat[] => [...beats, { done: result }];
  switch (outcome) {
    case 'tabletBack':
      return [...watch, { wait: 2500 }, { device: PLAIN }, log('Tablet seen: device', 'info'),
        ...end({ rescueOutcome: 'tabletBack', device: NOTE_MAX })];
    case 'backInEdl':
      return [...watch, { wait: 2500 }, { device: EDL }, log('Tablet seen in rescue (EDL) mode', 'info'),
        ...end({ rescueOutcome: 'backInEdl' })];
    case 'notSeen':
      return [...watch, { wait: 4000 }, log("Didn't see the tablet over USB; it may still be starting, or USB debugging is off", 'info'),
        ...end({ rescueOutcome: 'notSeen' })];
    case 'homeScreenSeen':
      // Waits on the user: the mock never sees the tablet.
      return [...watch, { prompt: TABLET_STARTING, branches: { homeScreen: end({ rescueOutcome: 'homeScreenSeen' }, log('You chose: homeScreen', 'info')) } }];
  }
};

/** Rescue: pick, cable guide, wait for the tablet in rescue mode, write, reset, watch it start. */
const rescueFlow = (o: { prefix?: Beat[]; waitEdl?: Beat[]; outcome?: RescueOutcome } = {}): Beat[] => [
  step('pickImage', 1),
  ...(o.prefix ?? []),
  log('Using boot_b.img from the selected backup'),
  { wait: 500 },
  step('cableGuide', 2),
  { wait: 2500 },
  step('waitEdl', 2),
  log('Waiting for Qualcomm HS-USB QDLoader 9008'),
  ...(o.waitEdl ?? [{ wait: 1500 }, { device: EDL }, log('Found Qualcomm HS-USB QDLoader 9008 (05c6:9008)')]),
  ...writeBoot(3, 'b', 'boot_b.img'), ...verifyWrite(3), ...reset(3),
  ...rescueWait(o.outcome ?? 'tabletBack'),
];

const driverPrompt = (kind: 'installDriver' | 'installUdev', what: string): Beat[] => [
  {
    prompt: {
      kind,
      options: [
        { id: 'install', label: `Install ${what}`, emphasis: 'primary' },
        { id: 'cancel', label: 'Cancel', emphasis: 'secondary' },
      ],
    },
    branches: {
      install: [log(`Installing ${what}`), { wait: 1500 }, log(`${what} installed`)],
      cancel: [{ cancelled: true }],
    },
  },
];

const unrootFlow = (): Beat[] => [
  ...checkDevice(1), ...enterEdl(2), ...writeBoot(2, 'b', 'boot_b.img'), ...verifyWrite(2), ...reset(2),
  ...waitBoot(3, PLAIN),
  { done: {} },
];

const reinstallFlow = (): Beat[] => [
  ...checkDevice(1),
  step('pushScripts', 2),
  log('adb push boot-hook.sh /data/adb/service.d/'),
  { wait: 800 },
  ...waitBoot(2, ROOTED), ...verifyRoot(2),
  { done: { magiskVersion: '30.7' } },
];

interface RootOpts { hasBackup: boolean; patch?: Beat[]; write?: Beat[]; reset?: Beat[]; boot?: Beat[] }
/** Root from group 3 on (patch to verifyRoot), shared with the reinstall hook-absent path. */
const rootCore = (o: RootOpts): Beat[] => [
  ...(o.patch ?? patch(3)), ...cableCheckPrompt(3), ...enterEdl(3),
  ...(o.write ?? writeBoot(3, 'b')), ...verifyWrite(3), ...(o.reset ?? reset(3)),
  ...(o.boot ?? waitBoot(4, ROOTED)), ...verifyRoot(4),
  { done: { backup: withPatched(o.hasBackup ? STOCK_BACKUP : NEW_BACKUP), magiskVersion: '30.7' } },
];
const rootFlow = (o: RootOpts): Beat[] => [
  ...checkDevice(1), ...safetyCopy(2, { alreadyHave: o.hasBackup }), ...rootCore(o),
];

const rootFailure = (message: string, detail: string): Beat[] => [
  ...checkDevice(1), ...safetyCopy(2, { alreadyHave: true }),
  step('patch', 3), log('Patching boot_b.img with Magisk 30.7'), { wait: 900 },
  log('magiskboot: unpack failed'),
  { fail: { code: 'patchFailed', message, detail, suggestedAction: 'retry' } },
];

const keepWaitingPrompt = (): Beat[] => [
  {
    prompt: {
      kind: 'keepWaitingOrRescue',
      options: [
        { id: 'keepWaiting', label: 'Keep waiting', emphasis: 'primary' },
        { id: 'goToRescue', label: 'Open rescue', emphasis: 'secondary' },
      ],
    },
    branches: { keepWaiting: () => [{ wait: 6000 }, ...keepWaitingPrompt()], goToRescue: [{ cancelled: true }] },
  },
];

/** The restart command keeps failing; the tablet stays in rescue mode until the user steps in. */
const stillInEdlReset = (): Beat[] => [
  step('reset', 3),
  log('edl reset --loader=prog_firehose_ddr.elf'),
  { wait: 800 },
  log('The restart command reported a problem: edl exited with code 1'),
  log('Trying the restart again (attempt 2 of 3)'),
  { wait: 800 },
  log('Trying the restart again (attempt 3 of 3)'),
  { wait: 800 },
  log('The tablet is still in rescue (EDL) mode'),
  {
    prompt: {
      kind: 'stillInEdl',
      options: [
        { id: 'retry', label: 'Try restarting again', emphasis: 'primary' },
        { id: 'heldPower', label: "I've restarted it", emphasis: 'secondary' },
      ],
    },
    branches: {
      retry: [log('edl reset --loader=prog_firehose_ddr.elf'), { wait: 800 }, { device: NONE }, log('The tablet has left rescue mode')],
      heldPower: [{ wait: 600 }, { device: NONE }, log('The tablet has left rescue mode')],
    },
  },
];

const hashMismatchWrite = (): Beat[] => [
  ...writeBoot(3, 'b'),
  step('verifyWrite', 3),
  log('Read-back sha256 differs from the written image'),
  {
    prompt: {
      kind: 'retryWrite',
      options: [
        { id: 'retry', label: 'Try again', emphasis: 'primary' },
        { id: 'cancel', label: 'Cancel', emphasis: 'secondary' },
      ],
    },
    branches: { retry: writeBoot(3, 'b'), cancel: [{ cancelled: true }] },
  },
];

const connectionLostWrite = (): Beat[] => [
  ...writeBoot(3, 'b'),
  step('verifyWrite', 3),
  log('USB error: No such device'),
  {
    prompt: {
      kind: 'connectionLostAfterWrite',
      options: [
        { id: 'restartedOk', label: 'It started normally', emphasis: 'primary' },
        { id: 'runAgain', label: 'Run Rescue again', emphasis: 'secondary' },
        { id: 'cancel', label: 'Stop', emphasis: 'secondary' },
      ],
    },
    branches: { restartedOk: [], runAgain: [{ cancelled: true }], cancel: [{ cancelled: true }] },
  },
];

const happyFlows = (hasBackup: boolean): Scenario['flows'] => ({
  backup: backupFlow(),
  importBackup: importFlow({ backup: IMPORTED_STOCK }),
  root: rootFlow({ hasBackup }),
  rescue: rescueFlow(),
  unroot: unrootFlow(),
  reinstallBootService: reinstallFlow(),
});

// ---- scenario table -----------------------------------------------------------------------

type Overrides = Partial<Omit<Scenario, 'name' | 'description' | 'device'>>;
const OK: Partial<Settings> = { hasRescueCable: null, acceptedRiskVersion: 1 };
const GATE: Partial<Settings> = { hasRescueCable: null, acceptedRiskVersion: null };

function make(name: string, description: string, device: DeviceStatus, o: Overrides = {}): Scenario {
  return { name, description, device, backups: [STOCK_BACKUP], settings: OK, flows: happyFlows(true), ...o };
}

const frozenFlow = (f: Partial<FlowState>): FlowState => ({
  flowId: 'mock-shot', kind: 'root', step: 'checkDevice', stepIndex: 1, stepCount: 4, status: 'running',
  prompt: null, error: null, result: null, writing: false, ...f,
});

const LONG_LOG = Array.from({ length: 40 }, (_, i) => `log line ${i + 1}: edl r boot_b readback.img --memory=ufs`);

const list: Scenario[] = [
  make('ready', 'Note Max connected, one stock backup, risk notice already accepted.', PLAIN),
  make('no-backup', 'Note Max connected, no backups yet; root takes the safety copy first.', PLAIN, {
    backups: [], settings: GATE, flows: happyFlows(false),
  }),
  make('accepted', 'Like ready, with the rescue cable answered too.', PLAIN, {
    settings: { hasRescueCable: true, acceptedRiskVersion: 1 },
  }),
  make('no-device', 'Nothing connected.', NONE, { settings: GATE }),
  make('plug-in', 'Nothing connected, then Allow on the tablet, then ready.', NONE, {
    settings: GATE,
    deviceScript: [
      { wait: 4000 }, { device: { kind: 'unauthorized' } }, { wait: 4000 }, { device: PLAIN },
    ],
  }),
  make('unauthorized', 'Tablet connected but USB debugging not allowed yet.', { kind: 'unauthorized' }, { settings: GATE }),
  make('edl', 'Tablet is already in rescue mode.', EDL, { settings: GATE }),
  make('unknown-model', 'A tablet Inkroot does not recognise.', adb(
    { ...NOTE_MAX, model: 'TabUltraCPro', profileId: null, displayName: 'TabUltraCPro' }, 'unknownModel',
  ), { settings: GATE }),
  make('unknown-firmware', 'A Note Max on firmware Inkroot has not seen.', adb(
    { ...NOTE_MAX, displayId: 'TKQ1.230721.003', incremental: '240', firmwareLabel: 'TKQ1.230721.003' }, 'unknownFirmware',
  ), { settings: GATE }),
  make('low-battery', 'Battery too low to start.', adb({ ...NOTE_MAX, battery: 18 }, 'lowBattery'), { settings: GATE }),
  make('rooted', 'Already rooted, stock backup on file.', ROOTED, { settings: GATE }),
  make('rooted-no-stock', 'Already rooted, only a rooted backup on file.', ROOTED, {
    settings: GATE, backups: [ROOTED_BACKUP],
  }),
  make('root-hash-mismatch', 'Root: the write check fails once, retry works.', PLAIN, {
    flows: { ...happyFlows(true), root: rootFlow({ hasBackup: true, write: hashMismatchWrite() }) },
  }),
  make('root-connection-lost', 'Root: the connection drops while checking the write.', PLAIN, {
    flows: { ...happyFlows(true), root: rootFlow({ hasBackup: true, write: connectionLostWrite() }) },
  }),
  make('root-boot-timeout', 'Root: the tablet takes too long to start.', PLAIN, {
    flows: {
      ...happyFlows(true),
      root: rootFlow({ hasBackup: true, boot: [step('waitBoot', 4), log('Waiting for the tablet to start up'), { wait: 5000 }, ...keepWaitingPrompt()] }),
    },
  }),
  make('root-still-in-edl', 'Root: the tablet stays in rescue mode after the restart command.', PLAIN, {
    flows: { ...happyFlows(true), root: rootFlow({ hasBackup: true, reset: stillInEdlReset() }) },
  }),
  make('root-patch-failed', 'Root: patching fails.', PLAIN, {
    flows: {
      ...happyFlows(true),
      root: rootFailure("We couldn't prepare the file for your tablet.", 'magiskboot unpack boot_b.img: exit status 1'),
    },
  }),
  make('root-busy', 'Root refuses to start: something else is running.', PLAIN, {
    startErrors: {
      root: { code: 'busy', message: 'Inkroot is already working on something. Please wait for it to finish.', detail: 'flow mock-0 is active', suggestedAction: null },
    },
  }),
  make('backup-edl-timeout', 'Backup: the tablet never shows up in rescue mode.', PLAIN, {
    flows: {
      ...happyFlows(true),
      backup: [
        ...checkDevice(1), step('enterEdl', 2), log('adb reboot edl'), { wait: 900 }, { device: NONE }, { wait: 6000 },
        edlNotFoundPrompt([
          { wait: 4000 },
          { fail: { code: 'edlTimeout', message: "Your tablet didn't switch to rescue mode.", detail: 'No 05c6:9008 device after 60 s', suggestedAction: 'retry' } },
        ]),
      ],
    },
  }),
  make('import-not-stock', 'Import: the file is not the original software.', PLAIN, {
    flows: {
      ...happyFlows(true),
      importBackup: [
        step('pickImage', 1), { wait: 600 }, step('stockCheck', 2), { wait: 900 },
        { fail: { code: 'notStock', message: "This file isn't the tablet's original software.", detail: 'boot image contains /sbin/magisk', suggestedAction: 'importBackup' } },
      ],
    },
  }),
  make('import-bad-image', 'Import: the file is not a boot image.', PLAIN, {
    flows: {
      ...happyFlows(true),
      importBackup: [
        step('pickImage', 1), { wait: 600 },
        { fail: { code: 'badImage', message: "We couldn't read that file as a tablet image.", detail: 'missing ANDROID! magic at offset 0', suggestedAction: 'importBackup' } },
      ],
    },
  }),
  make('import-rescue-only', 'Import: the file works for rescue but not for root.', NONE, {
    flows: { ...happyFlows(true), importBackup: importFlow({ backup: IMPORTED_RESCUE_ONLY }) },
  }),
  make('rescue-edl-not-found', 'Rescue: the tablet is not found at first.', NONE, {
    flows: {
      ...happyFlows(true),
      rescue: rescueFlow({
        waitEdl: [
          { wait: 6000 },
          edlNotFoundPrompt([{ wait: 1500 }, { device: EDL }, log('Found Qualcomm HS-USB QDLoader 9008 (05c6:9008)')]),
        ],
      }),
    },
  }),
  make('reinstall-hook-absent', 'Reinstall finds no boot service and switches to a full root.', ROOTED, {
    flows: {
      ...happyFlows(true),
      reinstallBootService: [
        ...checkDevice(1),
        log('Boot service not found on the tablet'),
        { state: { kind: 'root', stepIndex: 2, stepCount: 4 } },
        ...safetyCopy(2, { alreadyHave: true }), ...rootCore({ hasBackup: true }),
      ],
    },
  }),
  // Rescue endings (step 4 watches for the tablet; FlowResult.rescueOutcome).
  make('rescue-back', 'Rescue: the tablet starts and shows up over USB debugging.', EDL, {
    flows: { ...happyFlows(true), rescue: rescueFlow({ outcome: 'tabletBack' }) },
  }),
  make('rescue-not-seen', "Rescue: written and checked, but the tablet never shows up (USB debugging off).", EDL, {
    flows: { ...happyFlows(true), rescue: rescueFlow({ outcome: 'notSeen' }) },
  }),
  make('rescue-home-screen', 'Rescue: the user ends the wait with "I can see my home screen".', EDL, {
    flows: { ...happyFlows(true), rescue: rescueFlow({ outcome: 'homeScreenSeen' }) },
  }),
  make('rescue-back-in-edl', 'Rescue: the tablet drops back into rescue mode instead of starting.', EDL, {
    flows: { ...happyFlows(true), rescue: rescueFlow({ outcome: 'backInEdl' }) },
  }),
  make('windows-driver', 'Windows: rescue needs the USB driver first.', EDL, {
    appInfo: { os: 'windows', backupRoot: 'C:\\Users\\you\\Documents\\Inkroot' },
    flows: { ...happyFlows(true), rescue: rescueFlow({ prefix: driverPrompt('installDriver', 'the USB driver') }) },
  }),
  make('linux-udev', 'Linux: rescue needs a USB access rule first.', EDL, {
    appInfo: { os: 'linux', backupRoot: '/home/you/Documents/Inkroot' },
    flows: { ...happyFlows(true), rescue: rescueFlow({ prefix: driverPrompt('installUdev', 'the USB access rule') }) },
  }),
  make('shot-root-writing', 'Screenshot: root, mid-write.', PLAIN, {
    frozen: {
      flow: frozenFlow({ step: 'write', stepIndex: 3, writing: true }),
      progress: { flowId: 'mock-shot', done: 61865984, total: 100663296, label: 'boot_b' },
      logs: ['edl --memory=ufs w boot_b patched-boot.img'],
    },
  }),
  // Layout checks (e2e/layout.spec.ts): busy step and prompt screens with a long log to open in Details.
  make('shot-root-reset', 'Layout: root, restarting, with a long log.', PLAIN, {
    frozen: { flow: frozenFlow({ step: 'reset', stepIndex: 3 }), logs: LONG_LOG },
  }),
  make('shot-root-boot-timeout', "Layout: root, the tablet hasn't restarted yet, with a long log.", PLAIN, {
    frozen: {
      flow: frozenFlow({
        step: 'waitBoot', stepIndex: 4, status: 'waitingForUser',
        prompt: {
          id: 'shot-p', kind: 'keepWaitingOrRescue',
          options: [
            { id: 'keepWaiting', label: 'Keep waiting', emphasis: 'primary' },
            { id: 'goToRescue', label: 'Go to Rescue', emphasis: 'secondary' },
          ],
        },
      }),
      logs: LONG_LOG,
    },
  }),
  make('shot-root-still-in-edl', 'Layout: root, the tablet is still in rescue mode, with a long log.', PLAIN, {
    frozen: {
      flow: frozenFlow({
        step: 'reset', stepIndex: 3, status: 'waitingForUser',
        prompt: {
          id: 'shot-p', kind: 'stillInEdl',
          options: [
            { id: 'retry', label: 'Try restarting again', emphasis: 'primary' },
            { id: 'heldPower', label: "I've restarted it", emphasis: 'secondary' },
          ],
        },
      }),
      logs: LONG_LOG,
    },
  }),
  make('shot-rescue-cable', 'Screenshot: rescue, cable guide.', NONE, {
    frozen: { flow: frozenFlow({ kind: 'rescue', step: 'cableGuide', stepIndex: 2 }) },
  }),
  make('shot-rescue-waiting', 'Layout: rescue, written and checked, waiting for the tablet, with a long log.', NONE, {
    frozen: {
      flow: frozenFlow({ kind: 'rescue', step: 'waitBoot', stepIndex: 4, status: 'waitingForUser', prompt: { id: 'shot-p', ...TABLET_STARTING } }),
      logs: LONG_LOG,
    },
  }),
  make('shot-rescue-back', 'Layout: rescue done, the tablet is back (not rooted), with a long log.', PLAIN, {
    frozen: {
      flow: frozenFlow({ kind: 'rescue', step: 'done', stepIndex: 4, status: 'done', result: { rescueOutcome: 'tabletBack', device: NOTE_MAX } }),
      logs: LONG_LOG,
    },
  }),
  make('shot-rescue-not-seen', 'Layout: rescue done, the tablet was not seen, with a long log.', NONE, {
    frozen: {
      flow: frozenFlow({ kind: 'rescue', step: 'done', stepIndex: 4, status: 'done', result: { rescueOutcome: 'notSeen' } }),
      logs: LONG_LOG,
    },
  }),
  make('shot-rescue-back-in-edl', 'Layout: rescue done, the tablet went back into rescue mode, with a long log.', EDL, {
    frozen: {
      flow: frozenFlow({ kind: 'rescue', step: 'done', stepIndex: 4, status: 'done', result: { rescueOutcome: 'backInEdl' } }),
      logs: LONG_LOG,
    },
  }),
  make('shot-root-done', 'Screenshot: root finished.', ROOTED, {
    frozen: {
      flow: frozenFlow({
        step: 'done', stepIndex: 4, status: 'done',
        result: { backup: withPatched(STOCK_BACKUP), magiskVersion: '30.7' },
      }),
      logs: ['30.7:MAGISKSU'],
    },
  }),
];

export const SCENARIOS: Record<string, Scenario> = Object.fromEntries(list.map((s) => [s.name, s]));

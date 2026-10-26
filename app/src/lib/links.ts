export const REPO = 'https://github.com/n0dal/inkroot';
export const DISCLAIMER_URL = REPO + '/blob/main/DISCLAIMER.md';
export const NOTICES_URL = REPO + '/blob/main/THIRD_PARTY_NOTICES.md';
export const BOOT_SERVICE_DOCS_URL = REPO + '/blob/main/boot-service/README.md';
export const ACTIVITY_LAUNCHER_FDROID = 'https://f-droid.org/packages/de.szalkowski.activitylauncher/';
export const ACTIVITY_LAUNCHER_PLAY =
  'https://play.google.com/store/apps/details?id=de.szalkowski.activitylauncher';

export function newFirmwareIssueUrl(displayId: string, incremental: string): string {
  return REPO + '/issues/new?title=' + encodeURIComponent('Firmware support: ' + displayId + ' (' + incremental + ')');
}

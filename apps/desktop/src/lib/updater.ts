import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Signed updates from the public repo's GitHub Releases.
 *
 * Deliberately no modal dialogs anywhere in this flow. The launcher hides
 * itself when it loses focus, and a native dialog takes focus — so a
 * confirm prompt would make the window vanish underneath the very question
 * it asked. Everything is reported inline in the About tab instead, and the
 * startup check only raises a toast.
 *
 * The plugin verifies the signature itself; nothing here can weaken that.
 */

/** `check()` resolves to null when the running version is the latest. */
export type UpdateCheck =
  | { status: "current" }
  | { status: "available"; update: Update }
  /** The updater cannot run at all: a dev build, or a build made before the
   *  signing key was configured. Not an error worth alarming anyone with. */
  | { status: "unavailable"; reason: string };

export async function checkForUpdate(): Promise<UpdateCheck> {
  try {
    const update = await check();
    return update ? { status: "available", update } : { status: "current" };
  } catch (err) {
    return { status: "unavailable", reason: String(err) };
  }
}

/** Download and apply. The caller offers the restart afterwards — an
 *  applied update is not lost if the user keeps working for now. */
export async function installUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
}

export async function restartApp(): Promise<void> {
  await relaunch();
}

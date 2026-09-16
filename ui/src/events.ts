import { listen } from "@tauri-apps/api/event";
import type { InstanceStatus, ProgressEvent } from "./types";

export interface StatusEventPayload {
  id: string;
  status: InstanceStatus;
}

export interface ProgressEventPayload {
  id: string;
  event: ProgressEvent;
}

export const onInstanceStatus = (handler: (payload: StatusEventPayload) => void) =>
  listen<StatusEventPayload>("instance://status", (e) => handler(e.payload));

export const onInstanceProgress = (handler: (payload: ProgressEventPayload) => void) =>
  listen<ProgressEventPayload>("instance://progress", (e) => handler(e.payload));

export const onInstancesChanged = (handler: () => void) =>
  listen<null>("instances://changed", () => handler());

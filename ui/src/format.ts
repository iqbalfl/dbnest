import { isCommandError } from "./types";
import type { InstanceStatus, ProgressEvent } from "./types";

export function statusLabel(status: InstanceStatus): string {
  switch (status.state) {
    case "not_initialized":
      return "Belum diinisialisasi";
    case "stopped":
      return "Berhenti";
    case "starting":
      return "Memulai…";
    case "running":
      return status.pid ? `Berjalan (pid ${status.pid})` : "Berjalan";
    case "stopping":
      return "Menghentikan…";
    case "failed":
      return `Gagal: ${status.message}`;
  }
}

export function isRunning(status: InstanceStatus): boolean {
  return status.state === "running" || status.state === "starting";
}

export function progressLabel(event: ProgressEvent): string {
  if (typeof event === "string") {
    switch (event) {
      case "Preflight":
        return "Menjalankan preflight…";
      case "Initializing":
        return "Menginisialisasi data directory…";
      case "Starting":
        return "Menjalankan proses…";
      case "HealthCheck":
        return "Menunggu server siap…";
      case "Ready":
        return "Server siap";
    }
  }
  if ("Install" in event) {
    const installEvent = event.Install;
    if (typeof installEvent === "string") {
      switch (installEvent) {
        case "Verifying":
          return "Memverifikasi checksum…";
        case "Extracting":
          return "Mengekstrak binary…";
        case "Done":
          return "Instalasi selesai";
      }
    }
    if ("Downloading" in installEvent) {
      const { downloaded, total } = installEvent.Downloading;
      const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
      return total
        ? `Mengunduh ${mb(downloaded)}/${mb(total)} MB…`
        : `Mengunduh ${mb(downloaded)} MB…`;
    }
    return `Instalasi gagal: ${installEvent.Failed.message}`;
  }
  return `Gagal: ${event.Failed.message}`;
}

export function errorMessage(err: unknown): string {
  if (isCommandError(err)) {
    return err.hint ? `${err.message} (${err.hint})` : err.message;
  }
  if (err instanceof Error) return err.message;
  return String(err);
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

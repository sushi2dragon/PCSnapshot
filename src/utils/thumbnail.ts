import { convertFileSrc } from "@tauri-apps/api/core";

export function thumbnailUrl(path: string, revision: string): string {
  const url = convertFileSrc(path);
  const separator = url.includes("?") ? "&" : "?";
  return `${url}${separator}v=${encodeURIComponent(revision)}`;
}

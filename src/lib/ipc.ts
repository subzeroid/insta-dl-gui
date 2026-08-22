import { invoke } from "@tauri-apps/api/core";

export interface ConfigState {
  has_token: boolean;
  token_hint: string | null;
  dest_dir: string;
  sidecar: boolean;
}

export interface Balance {
  requests: number;
  rate: number | null;
  amount: number | null;
  currency: string | null;
}

export async function configState(): Promise<ConfigState> {
  return invoke("config_state");
}

export async function validateToken(token: string): Promise<Balance> {
  return invoke("validate_token", { token });
}

export async function getBalance(): Promise<Balance> {
  return invoke("get_balance");
}

export async function saveSettings(opts: { dest_dir?: string; sidecar?: boolean }): Promise<ConfigState> {
  return invoke("save_settings", { destDir: opts.dest_dir, sidecar: opts.sidecar });
}

export function formatBalance(b: Balance): string {
  const req = new Intl.NumberFormat("en", { notation: b.requests >= 1_000_000 ? "compact" : "standard" }).format(b.requests);
  const parts = [`${req} req`];
  if (b.amount !== null && b.currency !== null) {
    parts.push(`$${b.amount.toFixed(2)}`);
  }
  if (b.rate !== null) {
    parts.push(`${b.rate} rps`);
  }
  return parts.join(" · ");
}

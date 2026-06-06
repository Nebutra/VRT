export type VerificationMode = "dev" | "merge" | "release";

export interface VrtOptions {
  root?: string;
  bin?: string;
  env?: Record<string, string>;
}

export interface VerifyOptions extends VrtOptions {
  mode?: VerificationMode;
  full?: boolean;
  continue?: boolean;
}

export class VrtCommandError extends Error {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  command: string[];
}

export function verify<T = unknown>(options?: VerifyOptions): Promise<T>;
export function doctor<T = unknown>(options?: VrtOptions): Promise<T>;
export function explain<T = unknown>(options?: VrtOptions): Promise<T>;
export function bench<T = unknown>(options?: VrtOptions): Promise<T>;
export function runJson<T = unknown>(args: string[], options?: VrtOptions): Promise<T>;
export function resolveVrtBinary(explicitBin?: string): Promise<string>;

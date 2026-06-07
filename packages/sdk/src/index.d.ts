export type VerificationMode = "dev" | "merge" | "release";
export type TokenProfile = "standard" | "rtk" | "headroom";

export interface VrtOptions {
  root?: string;
  bin?: string;
  env?: Record<string, string>;
}

export interface VerifyOptions extends VrtOptions {
  mode?: VerificationMode;
  full?: boolean;
  continue?: boolean;
  tokenProfile?: TokenProfile;
}

export interface PlanOptions extends VrtOptions {
  mode?: VerificationMode;
  full?: boolean;
}

export class VrtCommandError extends Error {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  command: string[];
}

export function verify<T = unknown>(options?: VerifyOptions): Promise<T>;
export function plan<T = unknown>(options?: PlanOptions): Promise<T>;
export function doctor<T = unknown>(options?: VrtOptions): Promise<T>;
export function explain<T = unknown>(options?: VrtOptions): Promise<T>;
export function bench<T = unknown>(options?: VrtOptions): Promise<T>;
export function tokenDoctor<T = unknown>(options?: VrtOptions): Promise<T>;
export function tokenManifest<T = unknown>(options?: VrtOptions): Promise<T>;
export function tokenInstallRules(options?: VrtOptions): Promise<string>;
export function runText(args: string[], options?: VrtOptions): Promise<string>;
export function runJson<T = unknown>(args: string[], options?: VrtOptions): Promise<T>;
export function resolveVrtBinary(explicitBin?: string): Promise<string>;

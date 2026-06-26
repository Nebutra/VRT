import { User } from "./types";
export function greet(u: User): string { return `Hi ${u.name} <${u.email}>`; }

import { User } from "./types";
export function render(u: User): string { return u.id + ":" + u.name + ":" + u.email; }

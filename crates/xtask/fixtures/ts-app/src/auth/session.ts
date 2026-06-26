export interface Session {
  userId: string;
  token: string;
  expiresAt: number;
}

export function isExpired(s: Session, now: number): boolean {
  return s.expiresAt < now;
}

export interface Charge {
  amountCents: number;
  currency: string;
}

export function totalCents(charges: Charge[]): number {
  return charges.reduce((acc, c) => acc + c.amountCents, 0);
}

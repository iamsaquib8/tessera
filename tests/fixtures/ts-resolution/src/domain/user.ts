export interface Entity {
  id: string;
}

export class User implements Entity {
  constructor(public id: string, public name: string) {}

  display(): string {
    return this.name;
  }
}

export function point(x: number): number;
export function point(x: number, y: number): number;
export function point(x: number, y = 0): number {
  return x + y;
}

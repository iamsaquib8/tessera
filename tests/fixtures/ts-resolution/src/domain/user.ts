export interface Entity {
  id: string;
}

export class User implements Entity {
  constructor(public id: string, public name: string) {}

  display(): string {
    return this.name;
  }
}

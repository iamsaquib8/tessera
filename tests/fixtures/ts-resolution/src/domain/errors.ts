export class NotFound extends Error {}

export function notFound(id: string): NotFound {
  return new NotFound(id);
}

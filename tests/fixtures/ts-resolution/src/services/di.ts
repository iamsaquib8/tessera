export function Injectable(): ClassDecorator {
  return () => undefined;
}

export type Store = {
  save: (key: string) => Promise<void>;
};

export async function persist(store: Store) {
  await store.save("k");
}

export interface RequestGate {
  begin(): number;
  invalidate(): void;
  isCurrent(token: number): boolean;
  snapshot(): number;
}

export function createRequestGate(): RequestGate {
  let generation = 0;

  return {
    begin: () => ++generation,
    invalidate: () => {
      generation += 1;
    },
    isCurrent: (token) => token === generation,
    snapshot: () => generation,
  };
}

export function createExplorerRequestState() {
  return {
    autocomplete: createRequestGate(),
    profile: createRequestGate(),
    reels: createRequestGate(),
    stories: createRequestGate(),
  };
}

export async function runOnce<T>(
  active: Set<string>,
  key: string,
  action: () => Promise<T>,
): Promise<T | undefined> {
  if (active.has(key)) return undefined;
  active.add(key);
  try {
    return await action();
  } finally {
    active.delete(key);
  }
}

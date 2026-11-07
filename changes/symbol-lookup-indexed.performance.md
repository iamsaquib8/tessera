Looking up a symbol by name uses an index instead of scanning every symbol, so `verify --snippet` and `context` stay fast on large repositories; the index rebuilds once after the upgrade.

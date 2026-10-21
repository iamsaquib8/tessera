#include "store.h"

struct item {
    int id;
};

static int lookup(int id) { return id; }

int store_load(int id) { return lookup(id); }

#include "store.h"

int lookup(int id) { return id * 2; }

int main(void) { return store_load(lookup(1)); }

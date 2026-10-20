#include "store.hpp"

namespace app {
int Store::get(int id) const { return id; }

int load(int id) {
    Store s;
    return s.get(id);
}
}

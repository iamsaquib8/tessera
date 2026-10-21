#include "store.hpp"

int load(int x) { return x; }

int main() {
    auto* s = new app::Store();
    int r = s->get(app::load(1));
    delete s;
    return load(r);
}

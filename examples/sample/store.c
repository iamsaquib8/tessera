#include <stdio.h>
#include <string.h>

typedef struct {
    int id;
    char name[64];
} User;

static User load_user(int id) {
    User u;
    u.id = id;
    strncpy(u.name, "Ada", sizeof(u.name));
    return u;
}

User find_by_id(int id) {
    return load_user(id);
}

void render_user(int id) {
    User u = find_by_id(id);
    printf("%d: %s\n", u.id, u.name);
}

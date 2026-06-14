#include <string>
#include <unordered_map>

class UserStore {
public:
    explicit UserStore(std::string name) : name_(std::move(name)) {}

    std::string findById(int id) {
        return render(loadUser(id));
    }

private:
    std::string loadUser(int id) {
        return name_ + "#" + std::to_string(id);
    }

    std::string render(std::string user) {
        return "[" + user + "]";
    }

    std::string name_;
};

std::string render_user(UserStore& store, int id) {
    return store.findById(id);
}

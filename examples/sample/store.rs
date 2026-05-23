pub struct UserStore {
    pub name: String,
}

impl UserStore {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn find_by_id(&self, id: &str) -> String {
        let user = self.load_user(id);
        self.render(user)
    }

    fn load_user(&self, id: &str) -> String {
        format!("{}#{}", self.name, id)
    }

    fn render(&self, user: String) -> String {
        format!("[{}]", user)
    }
}

pub fn render_user(store: &UserStore, id: &str) -> String {
    store.find_by_id(id)
}
